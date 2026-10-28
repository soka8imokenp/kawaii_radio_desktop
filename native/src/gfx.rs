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
        }
    }
    pub fn full() -> Self {
        Self::plate_cut(0.0, 0.0, STAGE_W, STAGE_H, 1.0)
    }
    pub fn flat(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Self {
        Quad { rect: [x, y, w, h], uv: [0.0; 4], color, pivot: [0.0, 0.0], rot: 0.0, kind: 1.0, radius: 0.0, _pad: 0.0 }
    }
    pub fn glow(cx: f32, cy: f32, r: f32, color: [f32; 4]) -> Self {
        Quad { rect: [cx - r, cy - r, 2.0 * r, 2.0 * r], uv: [0.0; 4], color, pivot: [0.0, 0.0], rot: 0.0, kind: 2.0, radius: 0.0, _pad: 0.0 }
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
}

pub struct Gfx {
    pub window: Arc<Window>,
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
    textures: Vec<Texture>,
    white: usize,
    /// the stage's place in the window (px): offset and scale
    pub fit: (f32, f32, f32),
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
};
struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local: vec2<f32>,
    @location(3) kind: f32,
    @location(4) size: vec2<f32>,
    @location(5) radius: f32,
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
    return o;
}

@fragment
fn fs(o: Out) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, o.uv);   // first: sampling must happen in uniform control flow
    var mask = 1.0;
    if (o.radius > 0.0) {
        // signed distance to a rounded rect, in stage px; one px of antialiasing
        let half = o.size * 0.5;
        let q = abs(o.local * half) - half + vec2(o.radius);
        let d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - o.radius;
        mask = clamp(0.5 - d, 0.0, 1.0);
    }
    if (o.kind > 1.5) {
        let d = length(o.local);
        let a = clamp(1.0 - d, 0.0, 1.0);
        return vec4(o.color.rgb, o.color.a * a * a);
    }
    if (o.kind > 0.5) { return vec4(o.color.rgb, o.color.a * mask); }
    let c = t * o.color;
    return vec4(c.rgb, c.a * mask);
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
        let attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x2, 4 => Float32, 5 => Float32, 6 => Float32x2];
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
        let mut g = Gfx { window, surface, device, queue, config, pipe_alpha, pipe_add, view_buf, view_bind, tex_layout, sampler, textures: vec![], white: 0, fit: (0.0, 0.0, 1.0) };
        g.white = g.texture_rgba(1, 1, &[255, 255, 255, 255]);
        g.update_view();
        g
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w.max(1);
        self.config.height = h.max(1);
        self.surface.configure(&self.device, &self.config);
        self.update_view();
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
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) }],
        });
        self.textures.push(Texture { bind, size: (w, h) });
        self.textures.len() - 1
    }

    pub fn texture_image(&mut self, bytes: &[u8]) -> Option<usize> {
        let img = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        Some(self.texture_rgba(w, h, &img))
    }

    pub fn render(&mut self, batches: &[Batch]) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let bufs: Vec<_> = batches
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
            for (b, buf) in batches.iter().zip(&bufs) {
                let Some(buf) = buf else { continue };
                pass.set_pipeline(if b.add { &self.pipe_add } else { &self.pipe_alpha });
                pass.set_bind_group(1, &self.textures[b.tex.unwrap_or(self.white)].bind, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..6, 0..b.quads.len() as u32);
            }
        }
        self.queue.submit([enc.finish()]);
        self.window.pre_present_notify();
        self.queue.present(frame);
    }
}
