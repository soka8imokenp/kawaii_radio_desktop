В том промте был каркас ТЗ, но нейросети действительно часто халтурят и отвечают общими словами («добавьте Noise Texture»), если не расписать им конкретную математику нод и параметры шейдеров.

Ниже собран исчерпывающий, технически бескомпромиссный гайд в формате Markdown. Ты можешь скопировать его целиком в `.md` файл или просто отправить Claude Opus 5.5 — здесь прописаны точные сетапы нод Blender, физические значения IOR/Roughness, правила геометрии и математика шейдеров под Rust/wgpu.

---

```markdown
# Kawaii Radio: Comprehensive Technical & Art Direction Guide
**Target Stack:** Blender 4.x+ (Cycles / EEVEE Next) & Rust (wgpu / WGSL / egui / iced)  
**Art Style:** Retro-tactile skeuomorphism, Braun/Teenage Engineering hardware aesthetic, CRT phosphor luminescence, thermal print physics.

---

## 1. Physical Materials & Shader Setups (Blender Shader Editor)

### 1.1. Industrial Matte ABS Plastic (Chassis & Outer Shell)
*Goal: Eliminate CGI flatness, achieve microscopic molded grain and soft light scattering.*

* **Principled BSDF Base:**
  - Base Color: `#1A1D20` (Never use pure `#000000`; keep base albedo between 0.02 and 0.05).
  - Roughness: `0.52` base.
  - Subsurface Scattering (Weight): `0.03` — gives that tiny amount of light penetration seen in real dense polymers.
  - Subsurface Radius: `(0.1, 0.1, 0.1)`.
  - IOR: `1.46` to `1.50`.
* **Roughness Multi-Texture (Layered Micro-Variation):**
  - Connect a `Noise Texture` (Scale: `120.0`, Detail: `8.0`, Roughness: `0.65`) through a `Map Range` (From: 0.0–1.0, To: `0.45`–`0.60`) directly into **Roughness**.
  - Multiply with an `Ambient Occlusion` node (ColorRamp crushed) so internal crevices are slightly rougher and collect dust.
* **Normal / Bump Setup:**
  - Layer 1 (Mold Texture): `Noise Texture` (Scale: `350.0`, Detail: `12.0`, Distortion: `0.0`) -> `Bump` node (Strength: `0.025`, Distance: `0.001`).
  - Layer 2 (Edge Bevel): `Bevel` node (Radius: `0.002m`, Samples: `8`) plugged into the normal of the Bump node. This simulates a factory-molded rounded edge without ballooning polygon counts.

---

### 1.2. The Dual-Toggle Switch ("RADIO" vs "O'Z TANLOVIM")
*Goal: True physical socket clearance, mechanical spring-action depth, tactile ribbed grip.*

* **Physical Geometry Clearance:**
  - Both buttons must sit inside a single shared cavity (Trough).
  - Clearance tolerance: Model an air gap of exactly `0.6mm` to `1.0mm` between the button perimeter and the bezel wall.
  - Button state heights:
    - Active ("RADIO"): Protrudes `2.5mm` above the bezel plane. Top face has a `0.8mm` chamfer/bevel catching highlights.
    - Inactive ("O'Z TANLOVIM"): Depressed `-1.8mm` into the cavity. Deep ambient occlusion and contact shadows must obscure its bottom edge completely.
* **Ribbed Grip Geometry/Bump:**
  - For the 3 vertical grip grooves on the "RADIO" switch:
    - In Shader: `Wave Texture` (Bands, Sawtooth or Sine, Scale: `45.0`) -> `ColorRamp` (Constant step) -> `Bump` (Distance: `0.005`, Strength: `0.35`).
    - Or in Geometry: Boolean cutter with `0.4mm` cylindrical indents and soft bevels.

---

### 1.3. Anisotropic Machined Metal (Volume Potentiometer & Dial)
*Goal: Realistic radial brushed aluminum reflection matching studio audio consoles.*

* **Principled BSDF:**
  - Metallic: `1.0`
  - Base Color: `#D0D4D8`
  - Roughness: `0.28`
  - Anisotropic: `0.85`
  - Anisotropic Rotation: `0.25` (or mapped via Radial UV layout).
* **UV Coordinate Mapping:**
  - Unwrap the flat top face of the knob using **Project from View** into a circle.
  - Set the Texture Coordinate to `UV` and connect to an `Anisotropic Tangent` node (Direction: UV Map).
  - Micro-groove Bump: Add a `Wave Texture` (Rings, Scale: `220.0`) with very low bump strength (`0.015`) to create the physical lathe cutting rings.
* **Knurled Edge (Side Grip):**
  - Diamond or vertical serrations along the side cylinder: `Voronoi Texture` (Distance to Edge) -> `Bump` (Strength: `0.45`).

---

### 1.4. Multi-Layer CRT & Display Glass Optics
*Goal: Convex optical glass in front, physical phosphor/subpixel matrix behind.*

* **Outer Glass Pane (Separate Mesh in Front):**
  - Slightly curved convex shape (`1.5°` outward curve).
  - Transmission: `1.0`, Roughness: `0.04`, IOR: `1.52`.
  - Add a faint green/blue tint to Transmission Color: `#F0FFF8`.
  - Smudge & Fingerprint roughness mask (Roughness Map through ColorRamp to keep it subtle: max `0.15`).
  - Thin Film / AR Coating: Add a subtle `Layer Weight` (Facing: `0.92`) tinted purple/cyan to the coating coat.
* **Underlying Screen Grid (Waveform & Chat CRT):**
  - Emission Base: Dark olive green `#1A241A` (unlit) and vibrant phosphor `#86A34A` / `#56D856` (lit pixels).
  - Scanline Shader Node Group:
    - `Texture Coordinate (Window/UV)` -> `Separate XYZ (Y)` -> `Math (Multiply by screen_vertical_pixels)` -> `Math (Fract)` -> `ColorRamp (Crisp gradient)`.
    - Modulate the emission strength: Peak emission `3.5`, scanline trough `0.4`.

---

### 1.5. Thermal Receipt Paper & Subpixel Dot Matrix Printing
*Goal: Fibrous paper texture, heat bleed, low-res micro-jitter.*

* **Paper Subsurface & Roughness:**
  - Base Color: `#EDE9DF` (aged thermal white).
  - Subsurface Weight: `0.18`, Subsurface Radius: `(0.8, 0.7, 0.4)` (warm paper light transmission).
  - Roughness: `0.78`.
  - Microfiber Bump: `Musgrave Texture` or `Noise` (Scale: `600.0`, Detail: `14.0`, Strength: `0.02`).
* **Thermal Ink Shader:**
  - Never use razor-sharp vector SVGs or antialiased TrueType fonts.
  - Downsample font alpha to a low-res dot-matrix grid.
  - Run the text mask through a `Math (Greater Than)` node with a `Noise Texture` injected into the threshold to create microscopic jagged thermal burn defects and ink bleed.
  - Ink Color: `#1C191A` (Thermal ink is dark brownish-charcoal, never pure `#000000`).

---

## 2. Studio Lighting & Scene Contrast Configuration

* **Kill the Top-Left Sun Highlight:**
  - The previous visual issue was caused by an omnidirectional HDRI or an untargeted key Area Light casting a broad specular wash over the top-left casing.
  - Use **Area Lights with high Directional Spread** (Spread: `30°` to `45°` instead of default `180°`).
  - Position lights strictly from the front-left and bottom-right at grazing angles.
  - Use `Light Linking` (in Blender 4.x+) to restrict the main rim light so it hits the dial and side edges, but completely excludes the top-left chassis panel.
* **Internal LED Occlusion & Glow:**
  - LEDs (VU meter, active indicators) must have physical dark plastic borders/dividers between them to prevent internal light bleed inside the plastic case.
  - In Compositor: Use `Glare` (Fog Glow, Size: `5`, Mix: `-0.95`) combined with a `Cryptomatte` or `Emission Pass` to ensure only the lit diodes cast a soft halo without washing out the matte dark chassis.

---

## 3. Runtime Rust / Shader Implementation (WGSL/GLSL)

If you are rendering these effects dynamically in Rust (via `wgpu` or post-processing passes), apply this pipeline:

### 3.1. CRT Scanline & Phosphor Dot Fragment (WGSL)
```wgsl
@fragment
fn fs_crt(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    var color = textureSample(screen_texture, screen_sampler, uv).rgb;
    
    // 1. Horizontal Scanlines
    let scanline_count = 320.0;
    let scanline = sin(uv.y * scanline_count * 3.14159 * 2.0) * 0.5 + 0.5;
    color *= (0.75 + 0.25 * pow(scanline, 1.5));
    
    // 2. Micro Dot Matrix Mask (Phosphor triad)
    let dot_size = 480.0;
    let dot_x = sin(uv.x * dot_size * 3.14159 * 2.0) * 0.5 + 0.5;
    let dot_y = sin(uv.y * dot_size * 3.14159 * 2.0) * 0.5 + 0.5;
    color *= (0.85 + 0.15 * (dot_x * dot_y));
    
    // 3. Vignette & Inner Tube Falloff
    let d = distance(uv, vec2<f32>(0.5, 0.5));
    color *= clamp(1.0 - d * d * 0.7, 0.0, 1.0);
    
    return vec4<f32>(color, 1.0);
}

```

### 3.2. Thermal Receipt Dither / Burn Fragment (WGSL)

```wgsl
@fragment
fn fs_thermal_print(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let text_val = textureSample(text_mask_texture, text_sampler, uv).r;
    
    // High-frequency procedural burn jitter
    let noise = fract(sin(dot(uv, vec2<f32>(12.9898, 78.233))) * 43758.5453);
    let threshold = 0.48 + (noise - 0.5) * 0.08;
    
    let ink = step(threshold, text_val);
    let paper_color = vec3<f32>(0.93, 0.91, 0.87);
    let ink_color = vec3<f32>(0.11, 0.10, 0.10);
    
    let final_color = mix(paper_color, ink_color, ink);
    return vec4<f32>(final_color, 1.0);
}

```

---

## 4. Immediate Action Checklist for 3D Artist

1. [ ] Check the top-left Bezel: Isolate the area light and lower its spread to remove the specular haze.
2. [ ] Sink the "O'Z TANLOVIM" button by -1.8mm and establish a 0.8mm ambient occlusion clearance gap.
3. [ ] Add `Bevel` shader nodes (Radius: 2mm) to all button socket lips.
4. [ ] Plug high-frequency noise into the chassis Principled BSDF Roughness and micro-bump.
5. [ ] Replace smooth receipt typography with a genuine thermal matrix font with heat bleed.

```

---

С этим документом у Claude не останется пространства для абстрактных советов — тут есть конкретная топология, миллиметры зазоров, формулы шейдеров и значения шероховатости.

Ты собираешься запекать эти материалы в текстурные атласы (PBR-карты) для рендера в Rust, или интерфейс будет рендериться другим способом?

```