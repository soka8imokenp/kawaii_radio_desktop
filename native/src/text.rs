//! Live text (lyrics, LCD, headers…) with glyphon: shaped in stage px, rasterized at the window's real scale so it
//! stays crisp at every window size. Buffers are cached by their content and dropped when not used for a frame.

use glyphon::{Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight};
use std::collections::HashMap;

pub const MONO: &str = "Roboto Mono";
pub const VT: &str = "VT323";
pub const DOT: &str = "DotGothic16";
pub const MR: &str = "Rounded Mplus 1c";
pub const BAR: &str = "Barlow Semi Condensed";
pub const DELA: &str = "Dela Gothic One";

const FONTS: [&[u8]; 8] = [
    include_bytes!("../../app/ui/fonts/RobotoMono.ttf"),
    include_bytes!("../../app/ui/fonts/vt323.ttf"),
    include_bytes!("../../app/ui/fonts/dotgothic16.ttf"),
    include_bytes!("../../app/ui/fonts/mplus_r_md.ttf"),
    include_bytes!("../../app/ui/fonts/mplus_r_bd.ttf"),
    include_bytes!("../../app/ui/fonts/mplus_r_xb.ttf"),
    include_bytes!("../../app/ui/fonts/BarlowSemiCondensed-SemiBold.ttf"),
    include_bytes!("../../app/ui/fonts/dela.ttf"),
];

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// A run of text with its own colour / weight (the sung part of a line is brighter than the rest).
#[derive(Clone)]
pub struct Span {
    pub text: String,
    pub color: [u8; 4],
    pub weight: u16,
}

#[derive(Clone)]
pub struct Txt {
    pub spans: Vec<Span>,
    pub family: &'static str,
    /// font size and line height, stage px
    pub size: f32,
    pub line: f32,
    /// top-left (Left), top-centre (Center) or top-right (Right) of the text box, stage px
    pub x: f32,
    pub y: f32,
    /// wrap width (stage px); None = one line
    pub width: Option<f32>,
    pub align: Align,
    /// visible area, stage px [x0, y0, x1, y1]
    pub clip: [f32; 4],
    pub alpha: f32,
}

impl Txt {
    pub fn new(text: impl Into<String>, family: &'static str, size: f32, x: f32, y: f32, color: [u8; 4]) -> Self {
        Txt {
            spans: vec![Span { text: text.into(), color, weight: 400 }],
            family,
            size,
            line: size * 1.25,
            x,
            y,
            width: None,
            align: Align::Left,
            clip: [0.0, 0.0, 1920.0, 1080.0],
            alpha: 1.0,
        }
    }
    pub fn weight(mut self, w: u16) -> Self {
        for s in &mut self.spans {
            s.weight = w;
        }
        self
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
    pub fn wrap(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
    pub fn line(mut self, l: f32) -> Self {
        self.line = l;
        self
    }
    pub fn clip(mut self, c: [f32; 4]) -> Self {
        self.clip = c;
        self
    }
    pub fn alpha(mut self, a: f32) -> Self {
        self.alpha = a;
        self
    }
    fn key(&self) -> String {
        let mut k = format!("{}|{}|{}|{:?}|{}", self.family, self.size, self.line, self.width, self.align as u8);
        for s in &self.spans {
            k.push_str(&format!("|{}:{:?}:{}", s.text, s.color, s.weight));
        }
        k
    }
}

pub struct TextLayer {
    pub fs: FontSystem,
    swash: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    renderer: TextRenderer,
    cache: HashMap<String, (Buffer, u64)>,
    frame: u64,
}

impl TextLayer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let mut fs = FontSystem::new();
        for f in FONTS {
            fs.db_mut().load_font_data(f.to_vec());
        }
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        TextLayer { fs, swash: SwashCache::new(), viewport, atlas, renderer, cache: HashMap::new(), frame: 0 }
    }

    /// The width of a one-line text, stage px (for layout).
    pub fn measure(&mut self, t: &Txt) -> f32 {
        let key = t.key();
        self.ensure(t, &key);
        let b = &self.cache[&key].0;
        b.layout_runs().map(|r| r.line_w).fold(0.0, f32::max)
    }

    fn ensure(&mut self, t: &Txt, key: &str) {
        let frame = self.frame;
        if let Some(e) = self.cache.get_mut(key) {
            e.1 = frame;
            return;
        }
        let mut b = Buffer::new(&mut self.fs, Metrics::new(t.size, t.line));
        b.set_size(t.width, None);
        let spans: Vec<(&str, Attrs)> = t
            .spans
            .iter()
            .map(|s| (s.text.as_str(), Attrs::new().family(Family::Name(t.family)).weight(Weight(s.weight)).color(Color::rgba(s.color[0], s.color[1], s.color[2], s.color[3]))))
            .collect();
        let align = match t.align {
            Align::Left => None,
            Align::Center => Some(glyphon::cosmic_text::Align::Center),
            Align::Right => Some(glyphon::cosmic_text::Align::Right),
        };
        b.set_rich_text(spans, &Attrs::new().family(Family::Name(t.family)), Shaping::Advanced, align);
        b.shape_until_scroll(&mut self.fs, false);
        self.cache.insert(key.to_string(), (b, frame));
    }

    /// Lay the texts out for this frame (stage px -> window px with `fit` = (offset x, offset y, scale)).
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, size: (u32, u32), fit: (f32, f32, f32), texts: &[Txt]) {
        self.frame += 1;
        let keys: Vec<String> = texts.iter().map(|t| t.key()).collect();
        for (t, k) in texts.iter().zip(&keys) {
            self.ensure(t, k);
        }
        let (ox, oy, s) = fit;
        let mut areas = Vec::with_capacity(texts.len());
        for (t, k) in texts.iter().zip(&keys) {
            let b = &self.cache[k].0;
            let w = b.layout_runs().map(|r| r.line_w).fold(0.0, f32::max);
            // Center / Right without a wrap width: anchor the line at x
            let x = match (t.align, t.width) {
                (Align::Center, None) => t.x - w / 2.0,
                (Align::Right, None) => t.x - w,
                (Align::Center, Some(bw)) => t.x - bw / 2.0,
                (Align::Right, Some(bw)) => t.x - bw,
                _ => t.x,
            };
            let a = (t.alpha.clamp(0.0, 1.0) * 255.0) as u8;
            areas.push(TextArea {
                buffer: b,
                left: ox + x * s,
                top: oy + t.y * s,
                scale: s,
                bounds: TextBounds {
                    left: (ox + t.clip[0] * s) as i32,
                    top: (oy + t.clip[1] * s) as i32,
                    right: (ox + t.clip[2] * s).ceil() as i32,
                    bottom: (oy + t.clip[3] * s).ceil() as i32,
                },
                default_color: Color::rgba(255, 255, 255, a),
                custom_glyphs: &[],
            });
        }
        self.viewport.update(queue, Resolution { width: size.0, height: size.1 });
        let _ = self.renderer.prepare(device, queue, &mut self.fs, &mut self.atlas, &self.viewport, areas, &mut self.swash);
        let frame = self.frame;
        self.cache.retain(|_, v| frame - v.1 < 120);
    }

    pub fn render<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        let _ = self.renderer.render(&self.atlas, &self.viewport, pass);
    }

    pub fn trim(&mut self) {
        self.atlas.trim();
    }
}
