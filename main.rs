use eframe::egui;
use image::{DynamicImage, GenericImageView, imageops};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::thread;
use std::sync::{Arc, Mutex};

#[derive(Deserialize, Debug, Clone)]
struct ArtworkMetadata {
    title: String,
    artist: String,
    year: String,
}

#[derive(Clone)]
struct ArtworkInfo {
    path: PathBuf,
    metadata: ArtworkMetadata,
}

#[derive(Clone)]
struct ProcessedImage {
    main_image: DynamicImage,
    blurred_image: DynamicImage,
    metadata: ArtworkMetadata,
}

struct LoadedArtwork {
    texture: egui::TextureHandle,
    blurred_texture: egui::TextureHandle,
    metadata: ArtworkMetadata,
}

struct ArtSlideshowApp {
    artworks: Vec<ArtworkInfo>,
    current_index: usize,
    current_processed: Option<ProcessedImage>,
    next_processed: Arc<Mutex<Option<ProcessedImage>>>,
    current_textures: Option<LoadedArtwork>,
    last_change: Instant,
    slide_duration: Duration,
    loading_next: bool,
}

impl ArtSlideshowApp {
    fn new(folder_path: PathBuf) -> Self {
        let mut artworks = Vec::new();
        let entries = fs::read_dir(&folder_path).expect("Directory cannot be read");

        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "bmp" | "gif") {
                    let json_path = path.with_extension("json");
                    
                    let metadata = if json_path.exists() {
                        fs::read_to_string(&json_path)
                            .ok()
                            .and_then(|content| serde_json::from_str(&content).ok())
                            .unwrap_or_else(|| ArtworkMetadata {
                                title: "Unknown".to_string(),
                                artist: "Unknown".to_string(),
                                year: "Unknown".to_string(),
                            })
                    } else {
                        ArtworkMetadata {
                            title: path.file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string(),
                            artist: "Unknown".to_string(),
                            year: "Unknown".to_string(),
                        }
                    };

                    artworks.push(ArtworkInfo { path, metadata });
                }
            }
        }

        Self {
            artworks,
            current_index: 0,
            current_processed: None,
            next_processed: Arc::new(Mutex::new(None)),
            current_textures: None,
            last_change: Instant::now(),
            slide_duration: Duration::from_secs(10), // CHANGE TIME VALUE
            loading_next: false,
        }
    }

    fn process_image(path: &PathBuf, metadata: ArtworkMetadata) -> Option<ProcessedImage> {
        if let Ok(img) = image::open(path) {
            // image processing
            let (img_width, img_height) = img.dimensions();
            let max_dimension = 2048;
            let scale = if img_width.max(img_height) > max_dimension {
                max_dimension as f32 / img_width.max(img_height) as f32
            } else {
                1.0
            };
            
            let new_width = (img_width as f32 * scale) as u32;
            let new_height = (img_height as f32 * scale) as u32;
            let main_image = img.resize_exact(new_width, new_height, imageops::FilterType::Lanczos3);
            
            // background blur
            let blur_width = 640;
            let blur_height = 360;
            
            let blurred_small = img.resize_to_fill(blur_width, blur_height, imageops::FilterType::Lanczos3);
            let mut blurred = blurred_small.to_rgba8();
            
            // Multi-pass blur 
            for _ in 0..3 {
                blurred = Self::fast_box_blur(&blurred, 10);
            }
            
            // darken
            for pixel in blurred.pixels_mut() {
                pixel[0] = (pixel[0] as f32 * 0.6) as u8;
                pixel[1] = (pixel[1] as f32 * 0.6) as u8;
                pixel[2] = (pixel[2] as f32 * 0.6) as u8;
            }
            
            let blurred_image = DynamicImage::ImageRgba8(blurred);
            
            return Some(ProcessedImage {
                main_image,
                blurred_image,
                metadata,
            });
        }
        None
    }

    fn fast_box_blur(img: &image::ImageBuffer<image::Rgba<u8>, Vec<u8>>, radius: i32) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
        let (width, height) = img.dimensions();
        let mut output = img.clone();
        
        // Horizontal pass
        for y in 0..height {
            for x in 0..width {
                let mut r = 0u32;
                let mut g = 0u32;
                let mut b = 0u32;
                let mut count = 0u32;
                
                for dx in -radius..=radius {
                    let nx = (x as i32 + dx).clamp(0, width as i32 - 1) as u32;
                    let pixel = img.get_pixel(nx, y);
                    r += pixel[0] as u32;
                    g += pixel[1] as u32;
                    b += pixel[2] as u32;
                    count += 1;
                }
                
                let pixel = output.get_pixel_mut(x, y);
                pixel[0] = (r / count) as u8;
                pixel[1] = (g / count) as u8;
                pixel[2] = (b / count) as u8;
            }
        }
        
        let temp = output.clone();
        
        // Vertical pass
        for y in 0..height {
            for x in 0..width {
                let mut r = 0u32;
                let mut g = 0u32;
                let mut b = 0u32;
                let mut count = 0u32;
                
                for dy in -radius..=radius {
                    let ny = (y as i32 + dy).clamp(0, height as i32 - 1) as u32;
                    let pixel = temp.get_pixel(x, ny);
                    r += pixel[0] as u32;
                    g += pixel[1] as u32;
                    b += pixel[2] as u32;
                    count += 1;
                }
                
                let pixel = output.get_pixel_mut(x, y);
                pixel[0] = (r / count) as u8;
                pixel[1] = (g / count) as u8;
                pixel[2] = (b / count) as u8;
            }
        }
        
        output
    }

    fn load_next_in_background(&mut self) {
        if self.loading_next || self.artworks.len() <= 1 {
            return;
        }

        let next_index = (self.current_index + 1) % self.artworks.len();
        let next_info = self.artworks[next_index].clone();
        let next_processed = Arc::clone(&self.next_processed);
        
        self.loading_next = true;
        
        thread::spawn(move || {
            if let Some(processed) = Self::process_image(&next_info.path, next_info.metadata) {
                let mut next = next_processed.lock().unwrap();
                *next = Some(processed);
            }
        });
    }

    fn create_textures(ctx: &egui::Context, processed: &ProcessedImage, prefix: &str) -> LoadedArtwork {
        let texture = Self::image_to_texture(ctx, &processed.main_image, &format!("{}_main", prefix));
        let blurred_texture = Self::image_to_texture(ctx, &processed.blurred_image, &format!("{}_blur", prefix));
        
        LoadedArtwork {
            texture,
            blurred_texture,
            metadata: processed.metadata.clone(),
        }
    }

    fn image_to_texture(
        ctx: &egui::Context,
        image: &DynamicImage,
        name: &str,
    ) -> egui::TextureHandle {
        let size = [image.width() as usize, image.height() as usize];
        let pixels = image.to_rgba8();
        let pixels_flat: Vec<_> = pixels.as_flat_samples().as_slice().to_vec();

        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels_flat);

        ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
    }
}

impl eframe::App for ArtSlideshowApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.artworks.is_empty() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.heading("No images found in folder.");
                });
            });
            return;
        }

        // load first image
        if self.current_processed.is_none() {
            let current_info = &self.artworks[self.current_index];
            self.current_processed = Self::process_image(&current_info.path, current_info.metadata.clone());
            
            if let Some(processed) = &self.current_processed {
                self.current_textures = Some(Self::create_textures(ctx, processed, "current"));
            }
            
            // Start loading next in background
            self.load_next_in_background();
        }

        // verify if the next image had beed loaded
        if self.loading_next {
            let next_lock = self.next_processed.lock().unwrap();
            if next_lock.is_some() {
                self.loading_next = false;
            }
        }

        // Auto-advance slideshow only if it s done
        if self.last_change.elapsed() >= self.slide_duration {
            let next_lock = self.next_processed.lock().unwrap();
            
            if next_lock.is_some() {
                // index ++
                self.current_index = (self.current_index + 1) % self.artworks.len();
                self.last_change = Instant::now();
                
                self.current_processed = next_lock.clone();
                drop(next_lock);
                
                if let Some(processed) = &self.current_processed {
                    self.current_textures = Some(Self::create_textures(ctx, processed, "current"));
                }
                
                // remove next and load after
                {
                    let mut next = self.next_processed.lock().unwrap();
                    *next = None;
                }
                
                self.loading_next = false;
                self.load_next_in_background();
            }
        }

        // Render
        if let Some(loaded) = &self.current_textures {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::BLACK))
                .show(ctx, |ui| {
                    let screen_size = ui.available_size();

                    // Background blur FILL
                    let img = egui::Image::new(&loaded.blurred_texture)
                        .fit_to_exact_size(screen_size)
                        .maintain_aspect_ratio(false);
                    
                    ui.put(
                        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), screen_size),
                        img,
                    );

                    // image centred
                    let texture_size = loaded.texture.size();
                    let img_width = texture_size[0] as f32;
                    let img_height = texture_size[1] as f32;
                    
                    let scale_x = screen_size.x / img_width;
                    let scale_y = screen_size.y / img_height;
                    let scale = scale_x.min(scale_y);

                    let display_width = img_width * scale;
                    let display_height = img_height * scale;

                    let x_offset = (screen_size.x - display_width) / 2.0;
                    let y_offset = (screen_size.y - display_height) / 2.0;

                    ui.put(
                        egui::Rect::from_min_size(
                            egui::pos2(x_offset, y_offset),
                            egui::vec2(display_width, display_height),
                        ),
                        egui::Image::new(&loaded.texture)
                            .fit_to_exact_size(egui::vec2(display_width, display_height)),
                    );

                    // Text overlay
                    let text_margin = 30.0;
                    let text_y_base = screen_size.y - 120.0;

                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(text_margin - 15.0, text_y_base - 15.0),
                            egui::vec2(700.0, 110.0),
                        ),
                        8.0,
                        egui::Color32::from_black_alpha(200),
                    );

                    let line1 = format!("{} - {}", loaded.metadata.title, loaded.metadata.artist);
                    
                    ui.put(
                        egui::Rect::from_min_size(
                            egui::pos2(text_margin, text_y_base),
                            egui::vec2(650.0, 40.0),
                        ),
                        egui::Label::new(
                            egui::RichText::new(&line1)
                                .size(26.0)
                                .color(egui::Color32::WHITE)
                                .family(egui::FontFamily::Proportional),
                        ),
                    );

                    ui.put(
                        egui::Rect::from_min_size(
                            egui::pos2(text_margin, text_y_base + 45.0),
                            egui::vec2(650.0, 35.0),
                        ),
                        egui::Label::new(
                            egui::RichText::new(&loaded.metadata.year)
                                .size(22.0)
                                .color(egui::Color32::from_rgb(220, 220, 220))
                                .family(egui::FontFamily::Proportional),
                        ),
                    );
                });
        }

        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let folder_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from(r"C:\Users\david\Pictures\1880-1910")
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_fullscreen(true)
            .with_title("Art Slideshow"),
        ..Default::default()
    };

    eframe::run_native(
        "Art Slideshow",
        options,
        Box::new(|_cc| Ok(Box::new(ArtSlideshowApp::new(folder_path)))),
    )
}
