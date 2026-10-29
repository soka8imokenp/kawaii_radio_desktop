//! The GPU side: the device's rendered plates are textures, everything live is drawn as textured / tinted quads
//! in stage coordinates (1920 x 1080, the render's own pixel grid), scaled to the window with letterboxing.

use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

pub const STAGE_W: f32 = 1920.0;
pub const STAGE_H: f32 = 1080.0;

/// One quad: where it goes on the stage, which part of its texture it shows, a colour multiplier and a rotation
/// (radians, around `pivot`, in stage px).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Quad {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
    pub pivot: [f32; 2],
    pub rot: f32,
    /// 0 = textured, 1 = flat colour, 2 = a soft round glow (radial falloff)
    pub kind: f32,
    /// corner radius (stage px) of a rounded-rect mask; 0 = none
    pub radius: f32,
    pub _pad: f32,
    /// a rounded-rect clip in stage px (x0, y0, x1, y1; all 0 = none) and its corner radius
    pub mask: [f32; 4],
    pub mask_r: [f32; 2],
}

impl Quad {
    /// A part of a full-stage plate, cut to the same place on the stage.
    pub fn plate_cut(x0: f32, y0: f32, x1: f32, y1: f32, alpha: f32) -> Self {
        Quad {
            rect: [x0, y0, x1 - x0, y1 - y0],
            uv: [x0 / STAGE_W, y0 / STAGE_H, (x1 - x0) / STAGE_W, (y1 - y0) / STAGE_H],
            color: [1.0, 1.0, 1.0, alpha],
            pivot: [0.0, 0.0],
            rot: 0.0,
            kind: 0.0,
            radius: 0.0,
            _pad: 0.0,
            mask: [0.0; 4],
            mask_r: [0.0; 2],
        }
    }
    pub fn full() -> Self {
        Self::plate_cut(0.0, 0.0, STAGE_W, STAGE_H, 1.0)
    }
    pub fn flat(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Self {
        Quad { rect: [x, y, w, h], uv: [0.0; 4], color, pivot: [0.0, 0.0], rot: 0.0, kind: 1.0, radius: 0.0, _pad: 0.0, mask: [0.0; 4], mask_r: [0.0; 2] }
    }
    pub fn glow(cx: f32, cy: f32, r: f32, color: [f32; 4]) -> Self {
        Quad { rect: [cx - r, cy - r, 2.0 * r, 2.0 * r], uv: [0.0; 4], color, pivot: [0.0, 0.0], rot: 0.0, kind: 2.0, radius: 0.0, _pad: 0.0, mask: [0.0; 4], mask_r: [0.0; 2] }
    }
    /// A linear gradient from `a` to `b`, left -> right (`vertical` = top -> bottom).
    pub fn grad(x: f32, y: f32, w: f32, h: f32, a: [f32; 4], b: [f32; 4], vertical: bool) -> Self {
        Quad { rect: [x, y, w, h], uv: b, color: a, pivot: [0.0, 0.0], rot: 0.0, kind: 3.0, radius: 0.0, _pad: if vertical { 1.0 } else { 0.0 }, mask: [0.0; 4], mask_r: [0.0; 2] }
    }
    /// Dark scanlines in the window's own pixel rows (every 3rd row), so they never beat against the screen.
    pub fn scan(x: f32, y: f32, w: f32, h: f32, alpha: f32) -> Self {
        Quad { rect: [x, y, w, h], uv: [0.0; 4], color: [0.0, 0.0, 0.0, alpha], pivot: [0.0, 0.0], rot: 0.0, kind: 4.0, radius: 0.0, _pad: 0.0, mask: [0.0; 4], mask_r: [0.0; 2] }
    }
    /// Light outside a circle (centre, radius, softness in stage px): a curved reflection band.
    pub fn arc_light(x: f32, y: f32, w: f32, h: f32, c: [f32; 2], r: f32, soft: f32, aspect: f32, color: [f32; 4]) -> Self {
        Quad { rect: [x, y, w, h], uv: [c[0], c[1], r, soft], color, pivot: [0.0, 0.0], rot: 0.0, kind: 5.0, radius: 0.0, _pad: aspect, mask: [0.0; 4], mask_r: [0.0; 2] }
    }
    /// A rounded-rect rim of `width`, fading from `top` to `bottom`.
    pub fn rim(x: f32, y: f32, w: f32, h: f32, r: f32, width: f32, top: [f32; 4], bottom: [f32; 4]) -> Self {
        Quad { rect: [x, y, w, h], uv: bottom, color: top, pivot: [0.0, 0.0], rot: 0.0, kind: 6.0, radius: r, _pad: width, mask: [0.0; 4], mask_r: [0.0; 2] }
    }
    /// A dot mesh (pitch, dot radius in stage px).
    pub fn dots(x: f32, y: f32, w: f32, h: f32, pitch: f32, dot: f32, color: [f32; 4]) -> Self {
        Quad { rect: [x, y, w, h], uv: [pitch, dot, 0.0, 0.0], color, pivot: [0.0, 0.0], rot: 0.0, kind: 7.0, radius: 0.0, _pad: 0.0, mask: [0.0; 4], mask_r: [0.0; 2] }
    }
    /// A radial gradient filling the quad: `inner` in the middle -> `outer` at the edge.
    pub fn radial(x: f32, y: f32, w: f32, h: f32, inner: [f32; 4], outer: [f32; 4]) -> Self {
        Quad { rect: [x, y, w, h], uv: outer, color: inner, pivot: [0.0, 0.0], rot: 0.0, kind: 8.0, radius: 0.0, _pad: 0.0, mask: [0.0; 4], mask_r: [0.0; 2] }
    }
    /// Scanlines plus an aperture grille (vertical lines every 3rd device column) of alpha `cols`.
    pub fn scan2(x: f32, y: f32, w: f32, h: f32, rows: f32, cols: f32) -> Self {
        Quad { uv: [cols, 0.0, 0.0, 0.0], ..Self::scan(x, y, w, h, rows) }
    }
    /// CSS inset box-shadow: colour at the walls of the rounded rect, fading out `depth` px in.
    pub fn inset(x: f32, y: f32, w: f32, h: f32, r: f32, depth: f32, color: [f32; 4]) -> Self {
        Quad { rect: [x, y, w, h], uv: [0.0; 4], color, pivot: [0.0, 0.0], rot: 0.0, kind: 9.0, radius: r, _pad: depth, mask: [0.0; 4], mask_r: [0.0; 2] }
    }
    /// A tapered blade (top width = `taper` x bottom width), `top` -> `bottom` colour, edges blurred by `soft`.
    pub fn needle(x: f32, y: f32, w: f32, h: f32, taper: f32, top: [f32; 4], bottom: [f32; 4], soft: f32) -> Self {
        Quad { rect: [x, y, w, h], uv: bottom, color: top, pivot: [0.0, 0.0], rot: 0.0, kind: 10.0, radius: soft, _pad: taper, mask: [0.0; 4], mask_r: [0.0; 2] }
    }
    pub fn rounded(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }
    pub fn rotated(mut self, pivot: [f32; 2], rot: f32) -> Self {
        self.pivot = pivot;
        self.rot = rot;
        self
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct View {
    /// stage px -> clip space: clip = px * scale + offset
    scale: [f32; 2],
    offset: [f32; 2],
}

pub struct Texture {
    bind: wgpu::BindGroup,
    pub size: (u32, u32),
}

/// A draw call: quads that use one texture (`None` = no texture, flat colours and glows only).
pub struct Batch {
    pub tex: Option<usize>,
    pub quads: Vec<Quad>,
    /// additive blending (light), else normal alpha blending
    pub add: bool,
    /// draw only inside this stage rect
    pub clip: Option<[f32; 4]>,
}

/// The phosphor glow of the texts: they are drawn once more, in their glow colour, into a half-size target, blurred
/// there (a separable Gaussian, twice: a tight core and a wide bleed), and added onto the picture under the crisp text.
struct Bloom {
    size: (u32, u32),
    views: [wgpu::TextureView; 2],
    binds: [wgpu::BindGroup; 2],
}

const BLOOM: &str = r#"
struct Dir { d: vec2<f32>, k: f32, _p: f32 };
@group(0) @binding(0) var<uniform> dir: Dir;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs(@builtin(vertex_index) i: u32) -> V {
    let p = vec2(f32((i << 1u) & 2u), f32(i & 2u));
    var o: V;
    o.pos = vec4(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2(p.x, 1.0 - p.y);
    return o;
}
@fragment
fn blur(v: V) -> @location(0) vec4<f32> {
    var w = array<f32, 5>(0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
    var c = textureSample(tex, samp, v.uv) * w[0];
    for (var i = 1; i < 5; i++) {
        let o = dir.d * f32(i);
        c += (textureSample(tex, samp, v.uv + o) + textureSample(tex, samp, v.uv - o)) * w[i];
    }
    return c;
}
@fragment
fn comp(v: V) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, v.uv) * dir.k;
}
"#;

/// The last pass: the frame, copied to the window, seen through the convex tube glasses: inside each tube the picture
/// bulges (the middle a little magnified, nothing moves at the rim) and the colours part a hair towards the edges.
const POST: &str = r#"
struct Lens { r: array<vec4<f32>, 2>, p: array<vec4<f32>, 2>, size: vec4<f32> };
@group(0) @binding(0) var<uniform> L: Lens;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs(@builtin(vertex_index) i: u32) -> V {
    let p = vec2(f32((i << 1u) & 2u), f32(i & 2u));
    var o: V;
    o.pos = vec4(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2(p.x, 1.0 - p.y);
    return o;
}
@fragment
fn post(v: V) -> @location(0) vec4<f32> {
    let px = v.uv * L.size.xy;
    var ur = v.uv;
    var ug = v.uv;
    var ub = v.uv;
    for (var i = 0; i < 2; i++) {
        let r = L.r[i];
        if (r.z <= r.x) { continue; }
        let c = (r.xy + r.zw) * 0.5;
        let h = (r.zw - r.xy) * 0.5;
        let q = (px - c) / h;
        let a = abs(q);
        if (a.x < 1.0 && a.y < 1.0) {
            // a squircle norm: 0 in the middle, 1 at the rim (so the rim never moves)
            let n = pow(pow(a.x, 4.0) + pow(a.y, 4.0), 0.25);
            // only inside the glass's rounded outline, fading to nothing over its last 16 px (the bezel never moves)
            let rad = L.p[i].z;
            let dd = abs(px - c) - h + vec2(rad);
            let sd = length(max(dd, vec2(0.0))) + min(max(dd.x, dd.y), 0.0) - rad;
            let wgt = smoothstep(0.0, 16.0, -sd);
            let k = L.p[i].x * (1.0 - n * n) * wgt;
            let ca = L.p[i].y * n * n * wgt;
            let d = px - c;
            ug = (c + d * (1.0 - k)) / L.size.xy;
            ur = (c + d * (1.0 - k - ca)) / L.size.xy;
            ub = (c + d * (1.0 - k + ca)) / L.size.xy;
        }
    }
    let cr = textureSample(tex, samp, ur);
    let cg = textureSample(tex, samp, ug);
    let cb = textureSample(tex, samp, ub);
    return vec4(cr.r, cg.g, cb.b, 1.0);
}
"#;

pub struct Gfx {
    pub window: Arc<Window>,
    /// the graphics card and its API ("… · Vulkan"); soft = drawn by the processor (llvmpipe): no glow there
    pub gpu: String,
    pub soft: bool,
    surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pipe_alpha: wgpu::RenderPipeline,
    pipe_add: wgpu::RenderPipeline,
    view_buf: wgpu::Buffer,
    view_bind: wgpu::BindGroup,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    sampler_tiled: wgpu::Sampler,
    textures: Vec<Texture>,
    white: usize,
    /// the stage's place in the window (px): offset and scale
    pub fit: (f32, f32, f32),
    bloom: Option<Bloom>,
    blur_pipe: wgpu::RenderPipeline,
    comp_pipe: wgpu::RenderPipeline,
    /// uniforms: blur H, V (1 texel), H, V (2.5 texels), the composite's strength
    dirs: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    /// the frame is drawn here first, then copied to the window through the lenses
    scene: Option<((u32, u32), wgpu::TextureView, wgpu::BindGroup)>,
    post_pipe: wgpu::RenderPipeline,
    lens_buf: wgpu::Buffer,
    lens_bind: wgpu::BindGroup,
    /// the tube glasses (stage px rect) and their bulge / colour split
    pub lenses: [([f32; 4], [f32; 3]); 2],
}

const SHADER: &str = r#"
struct View { scale: vec2<f32>, offset: vec2<f32> };
@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Inst {
    @location(0) rect: vec4<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) pivot: vec2<f32>,
    @location(4) rot: f32,
    @location(5) kind: f32,
    @location(6) radius: vec2<f32>,
    @location(7) mask: vec4<f32>,
    @location(8) mask_r: vec2<f32>,
};
struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local: vec2<f32>,
    @location(3) kind: f32,
    @location(4) size: vec2<f32>,
    @location(5) radius: f32,
    @location(6) uvraw: vec4<f32>,
    @location(7) dir: f32,
    @location(8) stage: vec2<f32>,
    @location(9) mask: vec4<f32>,
    @location(10) mask_r: f32,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32, i: Inst) -> Out {
    var corners = array<vec2<f32>, 6>(vec2(0.,0.), vec2(1.,0.), vec2(0.,1.), vec2(1.,0.), vec2(1.,1.), vec2(0.,1.));
    let c = corners[vi];
    var p = i.rect.xy + c * i.rect.zw;
    if (i.rot != 0.0) {
        let d = p - i.pivot;
        let s = sin(i.rot); let k = cos(i.rot);
        p = i.pivot + vec2(d.x * k - d.y * s, d.x * s + d.y * k);
    }
    var o: Out;
    o.pos = vec4(p * view.scale + view.offset, 0.0, 1.0);
    o.uv = i.uv.xy + c * i.uv.zw;
    o.color = i.color;
    o.local = c * 2.0 - 1.0;
    o.kind = i.kind;
    o.size = i.rect.zw;
    o.radius = i.radius.x;
    o.uvraw = i.uv;
    o.dir = i.radius.y;
    o.stage = p;
    o.mask = i.mask;
    o.mask_r = i.mask_r.x;
    return o;
}

@fragment
fn fs(o: Out) -> @location(0) vec4<f32> {
    var c = shade(o);
    if (o.mask.z > o.mask.x) {
        let mc = (o.mask.xy + o.mask.zw) * 0.5;
        let mh = (o.mask.zw - o.mask.xy) * 0.5;
        let mq = abs(o.stage - mc) - mh + vec2(o.mask_r);
        let md = length(max(mq, vec2(0.0))) + min(max(mq.x, mq.y), 0.0) - o.mask_r;
        c.a = c.a * clamp(0.5 - md, 0.0, 1.0);
    }
    return c;
}

fn shade(o: Out) -> vec4<f32> {
    let t = textureSample(tex, samp, o.uv);   // first: sampling must happen in uniform control flow
    let fw = max(fwidth(o.local.x * o.size.x * 0.5), 0.001);   // one device pixel, in stage px
    // signed distance to the quad's rounded rect (stage px), and a 1 px antialiased mask
    let half = o.size * 0.5;
    let q = abs(o.local * half) - half + vec2(o.radius);
    let sd = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - o.radius;
    var mask = 1.0;
    if (o.radius > 0.0) { mask = clamp(0.5 - sd, 0.0, 1.0); }
    let sp = o.stage;
    switch (u32(o.kind + 0.5)) {
        case 0u: { let c = t * o.color; return vec4(c.rgb, c.a * mask); }
        case 1u: { return vec4(o.color.rgb, o.color.a * mask); }
        case 2u: {
            // a soft round glow (quadratic falloff)
            let a = clamp(1.0 - length(o.local), 0.0, 1.0);
            return vec4(o.color.rgb, o.color.a * a * a);
        }
        case 3u: {
            // a linear gradient color -> uvraw, along x (or y when dir = 1)
            let f = select(o.local.x, o.local.y, o.dir > 0.5) * 0.5 + 0.5;
            let c = mix(o.color, o.uvraw, f);
            return vec4(c.rgb, c.a * mask);
        }
        case 4u: {
            // scanlines in the window's own pixel rows, and a fainter aperture grille in its columns (uvraw.x)
            let ra = select(0.0, o.color.a, u32(o.pos.y) % 3u == 0u);
            let ca = select(0.0, o.uvraw.x, u32(o.pos.x) % 3u == 0u);
            return vec4(0.0, 0.0, 0.0, (1.0 - (1.0 - ra) * (1.0 - ca)) * mask);
        }
        case 5u: {
            // a curved glass reflection: the CSS radial-gradient stops (transparent 69%, .085 69.8%, .05 82%,
            // .02 94%, transparent 100%) of an ellipse (uvraw = centre x, y, x radius; dir = x/y aspect)
            let asp = select(1.0, o.dir, o.dir > 0.0);
            let f = length((sp - o.uvraw.xy) * vec2(1.0, asp)) / o.uvraw.z;
            var a = 0.0;
            if (f >= 0.69 && f < 0.698) { a = mix(0.0, 0.085, (f - 0.69) / 0.008); }
            else if (f >= 0.698 && f < 0.82) { a = mix(0.085, 0.05, (f - 0.698) / 0.122); }
            else if (f >= 0.82 && f < 0.94) { a = mix(0.05, 0.02, (f - 0.82) / 0.12); }
            else if (f >= 0.94 && f < 1.0) { a = mix(0.02, 0.0, (f - 0.94) / 0.06); }
            return vec4(o.color.rgb, o.color.a * a * mask);
        }
        case 6u: {
            // a rounded-rect rim of width dir (stage px), colour fading top (color) -> bottom (uvraw)
            let f = o.local.y * 0.5 + 0.5;
            let c = mix(o.color, o.uvraw, f);
            let a = clamp(0.5 - sd, 0.0, 1.0) * clamp(0.5 + sd + o.dir, 0.0, 1.0);
            return vec4(c.rgb, c.a * a);
        }
        case 7u: {
            // a phosphor dot mesh (uvraw.x = pitch, uvraw.y = dot radius, stage px)
            let g = fract(sp / o.uvraw.x) * o.uvraw.x - vec2(o.uvraw.x * 0.5);
            let a = clamp(o.uvraw.y - length(g) + 0.5, 0.0, 1.0);
            return vec4(o.color.rgb, o.color.a * a * mask);
        }
        case 9u: {
            // light / shadow inside a rounded rect, strongest at its wall, gone `dir` px in (CSS inset box-shadow)
            let k = clamp(1.0 + sd / o.dir, 0.0, 1.0);
            return vec4(o.color.rgb, o.color.a * k * k * mask);
        }
        case 10u: {
            // a needle: a blade narrowing to the top (dir = its top width / bottom width), colour top -> bottom,
            // its edges antialiased (or blurred by `radius` stage px, for its shadow)
            let f = o.local.y * 0.5 + 0.5;
            let hw = half.x * mix(o.dir, 1.0, f);
            let soft = max(o.radius, fw);
            let a = clamp((hw - abs(o.local.x * half.x)) / soft + 0.5, 0.0, 1.0);
            let c = mix(o.color, o.uvraw, f);
            return vec4(c.rgb, c.a * a);
        }
        default: {
            // radial gradient: color in the middle -> uvraw at the edge (ellipse filling the quad)
            let f = clamp(length(o.local), 0.0, 1.0);
            let c = mix(o.color, o.uvraw, f * f * (3.0 - 2.0 * f));
            return vec4(c.rgb, c.a * mask);
        }
    }
}
"#;

impl Gfx {
    pub async fn new(window: Arc<Window>) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(window.clone())).with_env());
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .expect("no GPU adapter");
        let info = adapter.get_info();
        let gpu = format!("{} · {:?}", info.name, info.backend);
        let soft = info.device_type == wgpu::DeviceType::Cpu;
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("device");
        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&view_layout), Some(&tex_layout)], immediate_size: 0 });
        let attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x2, 4 => Float32, 5 => Float32, 6 => Float32x2, 7 => Float32x4, 8 => Float32x2];
        let make = |blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Quad>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &attrs })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let add = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent::OVER,
        };
        let pipe_alpha = make(wgpu::BlendState::ALPHA_BLENDING);
        let pipe_add = make(add);
        let view_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::bytes_of(&View { scale: [0.0; 2], offset: [0.0; 2] }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let view_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &view_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: view_buf.as_entire_binding() }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        // the glow: blur passes (replace) and the composite (added onto the picture)
        let bshader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(BLOOM.into()) });
        let dir_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let blayout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&dir_layout), Some(&tex_layout)], immediate_size: 0 });
        let bmake = |entry: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&blayout),
                vertex: wgpu::VertexState { module: &bshader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                fragment: Some(wgpu::FragmentState {
                    module: &bshader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let blur_pipe = bmake("blur", None);
        let pshader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(POST.into()) });
        let post_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&blayout),
            vertex: wgpu::VertexState { module: &pshader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &pshader,
                entry_point: Some("post"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let lens_buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 80, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let lens_bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &dir_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: lens_buf.as_entire_binding() }] });
        let comp_pipe = bmake(
            "comp",
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
                alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            }),
        );
        let dirs = (0..5)
            .map(|_| {
                let b = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &dir_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: b.as_entire_binding() }] });
                (b, bg)
            })
            .collect();
        let sampler_tiled = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let mut g = Gfx { window, gpu, soft, surface, device, queue, config, pipe_alpha, pipe_add, view_buf, view_bind, tex_layout, sampler, sampler_tiled, textures: vec![], white: 0, fit: (0.0, 0.0, 1.0), bloom: None, blur_pipe, comp_pipe, dirs, scene: None, post_pipe, lens_buf, lens_bind, lenses: [([0.0; 4], [0.0; 3]); 2] };
        g.white = g.texture_rgba(1, 1, &[255, 255, 255, 255]);
        g.update_view();
        g
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w.max(1);
        self.config.height = h.max(1);
        self.surface.configure(&self.device, &self.config);
        self.update_view();
        self.bloom = None;
    }

    /// The glow targets at half the window's size (made again after a resize).
    fn bloom(&mut self) {
        let size = ((self.config.width / 2).max(1), (self.config.height / 2).max(1));
        if self.bloom.as_ref().is_some_and(|b| b.size == size) {
            return;
        }
        let mk = || {
            let t = self.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            t.create_view(&wgpu::TextureViewDescriptor::default())
        };
        let views = [mk(), mk()];
        let bind = |v: &wgpu::TextureView| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.tex_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(v) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) }],
            })
        };
        let binds = [bind(&views[0]), bind(&views[1])];
        let (tx, ty) = (1.0 / size.0 as f32, 1.0 / size.1 as f32);
        let k = std::env::var("KR_GLOW").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
        for (i, d) in [[tx, 0.0, 0.0, 0.0], [0.0, ty, 0.0, 0.0], [tx * 2.5, 0.0, 0.0, 0.0], [0.0, ty * 2.5, 0.0, 0.0], [0.0, 0.0, k, 0.0]].iter().enumerate() {
            self.queue.write_buffer(&self.dirs[i].0, 0, bytemuck::cast_slice(d));
        }
        self.bloom = Some(Bloom { size, views, binds });
    }

    /// Fit the 16:9 stage into the window (letterbox), and the matching stage -> clip transform.
    fn update_view(&mut self) {
        let (w, h) = (self.config.width as f32, self.config.height as f32);
        let s = (w / STAGE_W).min(h / STAGE_H);
        let (ox, oy) = ((w - STAGE_W * s) / 2.0, (h - STAGE_H * s) / 2.0);
        self.fit = (ox, oy, s);
        let v = View { scale: [2.0 * s / w, -2.0 * s / h], offset: [2.0 * ox / w - 1.0, 1.0 - 2.0 * oy / h] };
        self.queue.write_buffer(&self.view_buf, 0, bytemuck::bytes_of(&v));
    }

    /// Window px -> stage px.
    pub fn to_stage(&self, x: f64, y: f64) -> (f32, f32) {
        let (ox, oy, s) = self.fit;
        ((x as f32 - ox) / s, (y as f32 - oy) / s)
    }

    pub fn texture_rgba(&mut self, w: u32, h: u32, rgba: &[u8]) -> usize {
        self.texture_ex(w, h, rgba, false)
    }

    /// A texture that tiles (uv outside 0..1 repeats it): the app's own surface patterns.
    pub fn texture_tiled(&mut self, w: u32, h: u32, rgba: &[u8]) -> usize {
        self.texture_ex(w, h, rgba, true)
    }

    fn texture_ex(&mut self, w: u32, h: u32, rgba: &[u8], tiled: bool) -> usize {
        let mips = (w.max(h) as f32).log2().floor() as u32 + 1;
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // mip chain on the CPU (box filter): the plates are drawn smaller than their 4K size most of the time
        let mut img = image::RgbaImage::from_raw(w, h, rgba.to_vec()).expect("rgba size");
        for level in 0..mips {
            let (lw, lh) = img.dimensions();
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: level, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &img,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * lw), rows_per_image: Some(lh) },
                wgpu::Extent3d { width: lw, height: lh, depth_or_array_layers: 1 },
            );
            if level + 1 < mips {
                img = image::imageops::resize(&img, (lw / 2).max(1), (lh / 2).max(1), image::imageops::FilterType::Triangle);
            }
        }
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.tex_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(if tiled { &self.sampler_tiled } else { &self.sampler }) }],
        });
        self.textures.push(Texture { bind, size: (w, h) });
        self.textures.len() - 1
    }

    pub fn texture_image(&mut self, bytes: &[u8]) -> Option<usize> {
        let img = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        Some(self.texture_rgba(w, h, &img))
    }

    /// `bloom`: lights that also glow — drawn into the glow target too (blurred there with the texts' glow).
    pub fn render(&mut self, batches: &[Batch], text: Option<(&mut crate::text::TextLayer, &[crate::text::Txt])>, over: &[Batch], bloom: &[Batch]) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
        };
        let out = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let size = (self.config.width, self.config.height);
        if self.scene.as_ref().is_none_or(|s| s.0 != size) {
            let t = self.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let v = t.create_view(&wgpu::TextureViewDescriptor::default());
            let b = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.tex_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&v) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) }],
            });
            self.scene = Some((size, v, b));
        }
        // the lenses in window px
        let (ox, oy, sc) = self.fit;
        let mut lu = [0f32; 20];
        for (i, (r, k)) in self.lenses.iter().enumerate() {
            lu[i * 4..i * 4 + 4].copy_from_slice(&[ox + r[0] * sc, oy + r[1] * sc, ox + r[2] * sc, oy + r[3] * sc]);
            lu[8 + i * 4] = k[0];
            lu[8 + i * 4 + 1] = k[1];
            lu[8 + i * 4 + 2] = k[2] * sc;
        }
        lu[16] = size.0 as f32;
        lu[17] = size.1 as f32;
        self.queue.write_buffer(&self.lens_buf, 0, bytemuck::cast_slice(&lu));
        let view = self.scene.as_ref().unwrap().1.clone();
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let mut text = text;
        let mut glow = false;
        if let Some((tl, txts)) = text.as_mut() {
            let half = ((self.config.width / 2).max(1), (self.config.height / 2).max(1));
            glow = tl.prepare(&self.device, &self.queue, (self.config.width, self.config.height), half, self.fit, txts);
        }
        // a processor-drawn picture skips the blurred glow (four extra full passes)
        let glow = !self.soft && (glow || bloom.iter().any(|b| !b.quads.is_empty()));
        let bloom_bufs: Vec<_> = bloom
            .iter()
            .map(|b| (!b.quads.is_empty()).then(|| self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&b.quads), usage: wgpu::BufferUsages::VERTEX })))
            .collect();
        if glow {
            self.bloom();
            let b = self.bloom.as_ref().unwrap();
            let pass_to = |enc: &mut wgpu::CommandEncoder, v: &wgpu::TextureView| -> wgpu::RenderPass<'static> {
                enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: v,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime()
            };
            {
                let mut p = pass_to(&mut enc, &b.views[0]);
                if let Some((tl, _)) = text.as_ref() {
                    tl.render_glow(&mut p);
                }
                p.set_bind_group(0, &self.view_bind, &[]);
                for (bt, buf) in bloom.iter().zip(&bloom_bufs) {
                    let Some(buf) = buf else { continue };
                    p.set_pipeline(&self.pipe_add);
                    p.set_bind_group(1, &self.textures[bt.tex.unwrap_or(self.white)].bind, &[]);
                    p.set_vertex_buffer(0, buf.slice(..));
                    p.draw(0..6, 0..bt.quads.len() as u32);
                }
            }
            // A -> B (H), B -> A (V), twice: 1 texel apart, then 2.5 texels apart
            for (k, (src, dst)) in [(0usize, 1usize), (1, 0), (0, 1), (1, 0)].into_iter().enumerate() {
                let mut p = pass_to(&mut enc, &b.views[dst]);
                p.set_pipeline(&self.blur_pipe);
                p.set_bind_group(0, &self.dirs[k].1, &[]);
                p.set_bind_group(1, &b.binds[src], &[]);
                p.draw(0..3, 0..1);
            }
        }
        let all: Vec<&Batch> = batches.iter().chain(over.iter()).collect();
        let bufs: Vec<_> = all
            .iter()
            .map(|b| (!b.quads.is_empty()).then(|| self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&b.quads), usage: wgpu::BufferUsages::VERTEX })))
            .collect();
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.012, g: 0.012, b: 0.016, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.view_bind, &[]);
            for (n, (b, buf)) in all.iter().zip(&bufs).enumerate() {
                if n == batches.len() {
                    if let (true, Some(bl)) = (glow, self.bloom.as_ref()) {
                        pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
                        pass.set_pipeline(&self.comp_pipe);
                        pass.set_bind_group(0, &self.dirs[4].1, &[]);
                        pass.set_bind_group(1, &bl.binds[0], &[]);
                        pass.draw(0..3, 0..1);
                    }
                    if let Some((tl, _)) = text.as_ref() {
                        tl.render(&mut pass);
                    }
                    pass.set_bind_group(0, &self.view_bind, &[]);
                }
                let Some(buf) = buf else { continue };
                pass.set_pipeline(if b.add { &self.pipe_add } else { &self.pipe_alpha });
                let (ox, oy, s) = self.fit;
                let (sw, sh) = (self.config.width as f32, self.config.height as f32);
                match b.clip {
                    Some([cx0, cy0, cx1, cy1]) => {
                        let x = (ox + cx0 * s).clamp(0.0, sw);
                        let y = (oy + cy0 * s).clamp(0.0, sh);
                        let x2 = (ox + cx1 * s).clamp(0.0, sw);
                        let y2 = (oy + cy1 * s).clamp(0.0, sh);
                        if x2 - x < 1.0 || y2 - y < 1.0 {
                            continue;
                        }
                        pass.set_scissor_rect(x as u32, y as u32, (x2 - x) as u32, (y2 - y) as u32);
                    }
                    None => pass.set_scissor_rect(0, 0, self.config.width, self.config.height),
                }
                pass.set_bind_group(1, &self.textures[b.tex.unwrap_or(self.white)].bind, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..6, 0..b.quads.len() as u32);
            }
        }
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.post_pipe);
            pass.set_bind_group(0, &self.lens_bind, &[]);
            pass.set_bind_group(1, &self.scene.as_ref().unwrap().2, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([enc.finish()]);
        self.window.pre_present_notify();
        self.queue.present(frame);
    }
}
