use std::collections::HashMap;
use std::path::PathBuf;

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

pub struct ThumbCache {
    textures: HashMap<String, TextureHandle>,
    pending: HashMap<String, String>,
    placeholder: TextureHandle,
}

impl ThumbCache {
    pub fn new(ctx: &Context) -> Self {
        Self {
            textures: HashMap::new(),
            pending: HashMap::new(),
            placeholder: load_placeholder(ctx),
        }
    }

    pub fn get(&self, id: &str) -> TextureHandle {
        self.textures.get(id).cloned().unwrap_or_else(|| self.placeholder.clone())
    }

    pub fn insert(&mut self, ctx: &Context, id: &str, path: PathBuf) {
        if let Ok(bytes) = std::fs::read(&path)
            && let Ok(source) = image::load_from_memory(&bytes).map(|i| i.into_rgba8()) {
                let (width, height) = source.dimensions();
                let size = [width as usize, height as usize];
                let image = ColorImage::from_rgba_unmultiplied(size, source.as_raw());
                let texture = ctx.load_texture(
                    format!("thumb-{id}"),
                    image,
                    TextureOptions::LINEAR,
                );
                self.textures.insert(id.to_string(), texture);
            }
    }

    pub fn request_pending(&mut self, id: &str) -> bool {
        !self.textures.contains_key(id) && !self.pending.contains_key(id)
    }

    pub fn mark_pending(&mut self, id: &str, url: String) {
        self.pending.insert(id.to_string(), url);
    }

    pub fn clear_pending(&mut self, id: &str) {
        self.pending.remove(id);
    }

    pub fn forget_all(&mut self) {
        self.textures.clear();
        self.pending.clear();
    }

    pub fn cached_count(&self) -> usize {
        self.textures.len()
    }
}

fn load_placeholder(ctx: &Context) -> TextureHandle {
    let (w, h) = (320u32, 180u32);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let stripe = ((x / 16 + y / 16) % 2) == 0;
            let base = if stripe { 34 } else { 40 };
            pixels.extend_from_slice(&[base, base + 4, base + 10, 255]);
        }
    }
    let image = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels);
    ctx.load_texture("placeholder", image, TextureOptions::NEAREST)
}
