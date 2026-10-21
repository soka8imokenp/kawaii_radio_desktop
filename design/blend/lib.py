"""Shared Blender helpers for the KR-01 DESK renders: scene setup, materials (two finishes), rounded boxes, text, pixel icons."""
import bpy, bmesh, math, os
from mathutils import Vector, Matrix

HERE = os.path.dirname(os.path.abspath(__file__))
FONTS = os.path.join(HERE, '..', 'fonts')
TEX = os.path.join(HERE, 'tex')

W, H = 1920, 1080


def P(x, y):
    """UI pixels → world XY (1 unit = 100 px, origin at the frame centre)."""
    return x / 100 - W / 200, H / 200 - y / 100


def hexc(h):
    h = h.lstrip('#')
    return tuple((int(h[i:i + 2], 16) / 255) ** 2.2 for i in (0, 2, 4))


# ───────────────────────────── finishes (the phone's Theme.kt, extended)
THEMES = {
    'light': dict(
        chassis='#e9ebef', tray='#dde0e6', dots='#c9cdd5', trayDots='#c3c7d0',
        key='#f8f8fa', keyStation='#2f3137', ink='#4b5563', inkD='#1f2329', inkSoft='#7b818c',
        bay='#26282d', bayDots='#3a3d44', well='#121315', rim='#f3f4f6',
        bubble='#f8f8fa', paper='#f4f1ea', lcdFrame='#363d2e',
        lcdA='#d8e1b3', lcdB='#a6b385', lcdEmit=0.22,
        world=(0.78, 0.8, 0.85), key_light=1700, fill=450, top=250,
    ),
    'dark': dict(
        chassis='#1b1d21', tray='#131417', dots='#25282d', trayDots='#1d1f23',
        key='#2c2f35', keyStation='#18191c', ink='#b9bec7', inkD='#eef0f3', inkSoft='#80868f',
        bay='#101114', bayDots='#1c1e22', well='#08090a', rim='#d9dce1',
        bubble='#2a2d33', paper='#e9e5db', lcdFrame='#22271c',
        lcdA='#bfcb93', lcdB='#8b9866', lcdEmit=0.32,
        world=(0.3, 0.32, 0.36), key_light=1500, fill=420, top=200,
    ),
}


class Ctx:
    """Everything a build needs: the scene, the theme and its materials."""

    def __init__(self, theme='light', w=W, h=H, samples=256, ortho=True):
        bpy.ops.wm.read_factory_settings(use_empty=True)
        self.T = THEMES[theme]
        self.theme = theme
        sc = self.sc = bpy.context.scene
        sc.render.engine = 'CYCLES'
        sc.render.resolution_x, sc.render.resolution_y = w, h
        sc.render.resolution_percentage = 100
        sc.cycles.samples = samples
        sc.cycles.use_denoising = True
        sc.cycles.max_bounces = 8
        sc.cycles.transmission_bounces = 8
        sc.view_settings.view_transform = 'Standard'
        sc.view_settings.look = 'None'
        self._gpu()
        world = bpy.data.worlds.new('w')
        sc.world = world
        world.use_nodes = True
        bg = world.node_tree.nodes['Background']
        bg.inputs['Color'].default_value = (*self.T['world'], 1)
        bg.inputs['Strength'].default_value = 0.22
        if ortho:
            cd = bpy.data.cameras.new('cam')
            cd.type = 'ORTHO'
            cd.ortho_scale = w / 100
            cd.clip_end = 100
            cam = bpy.data.objects.new('cam', cd)
            cam.location = (0, 0, 20)
            sc.collection.objects.link(cam)
            sc.camera = cam
        self._fonts = {}
        self._mats()

    def _gpu(self):
        try:
            prefs = bpy.context.preferences.addons['cycles'].preferences
            for kind in ('OPTIX', 'CUDA'):
                try:
                    prefs.compute_device_type = kind
                    prefs.get_devices()
                    if any(d.type == kind for d in prefs.devices):
                        for d in prefs.devices:
                            d.use = d.type == kind
                        self.sc.cycles.device = 'GPU'
                        return
                except Exception:
                    pass
        except Exception:
            pass

    # ── lights
    def area(self, name, loc, size, energy, color=(1, 1, 1), target=(0, 0, 0), camera=False, glossy=True, shape='SQUARE', size_y=None):
        d = bpy.data.lights.new(name, 'AREA')
        d.size = size
        d.energy = energy
        d.color = color
        if shape != 'SQUARE':
            d.shape = shape
            d.size_y = size_y or size
        o = bpy.data.objects.new(name, d)
        o.visible_camera = camera
        o.visible_glossy = glossy
        o.location = loc
        self.sc.collection.objects.link(o)
        o.rotation_euler = (Vector(target) - Vector(loc)).to_track_quat('-Z', 'Y').to_euler()
        return o

    def spot(self, name, loc, energy, target, size_deg=40, blend=0.6, color=(1, 1, 1), radius=0.3):
        d = bpy.data.lights.new(name, 'SPOT')
        d.energy = energy
        d.spot_size = math.radians(size_deg)
        d.spot_blend = blend
        d.color = color
        d.shadow_soft_size = radius
        o = bpy.data.objects.new(name, d)
        o.visible_camera = False
        o.location = loc
        self.sc.collection.objects.link(o)
        o.rotation_euler = (Vector(target) - Vector(loc)).to_track_quat('-Z', 'Y').to_euler()
        return o

    def studio(self):
        T = self.T
        self.area('key', (-9, 9, 11), 9, T['key_light'], (1.0, 0.98, 0.95))
        self.area('fill', (10, -6, 9), 12, T['fill'], (0.92, 0.95, 1.0))
        self.area('top', (0, 0, 14), 18, T['top'])

    # ── materials
    def mat(self, name, color, rough=0.5, metal=0.0, coat=0.0, emit=None, estr=0.0, trans=0.0, ior=1.45, aniso=0.0, bump=None, spec=0.5, alpha=1.0, sss=0.0):
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        nt = m.node_tree
        b = nt.nodes['Principled BSDF']
        b.inputs['Base Color'].default_value = (*color, 1)
        b.inputs['Roughness'].default_value = rough
        b.inputs['Metallic'].default_value = metal
        b.inputs['Coat Weight'].default_value = coat
        b.inputs['Coat Roughness'].default_value = 0.08
        b.inputs['Transmission Weight'].default_value = trans
        b.inputs['IOR'].default_value = ior
        b.inputs['Anisotropic'].default_value = aniso
        b.inputs['Specular IOR Level'].default_value = spec
        b.inputs['Alpha'].default_value = alpha
        b.inputs['Subsurface Weight'].default_value = sss
        if emit:
            b.inputs['Emission Color'].default_value = (*emit, 1)
            b.inputs['Emission Strength'].default_value = estr
        if bump:
            scale, strength = bump
            n = nt.nodes.new('ShaderNodeTexNoise')
            n.inputs['Scale'].default_value = scale
            n.inputs['Detail'].default_value = 8
            bp = nt.nodes.new('ShaderNodeBump')
            bp.inputs['Strength'].default_value = strength
            bp.inputs['Distance'].default_value = 0.02
            nt.links.new(n.outputs['Fac'], bp.inputs['Height'])
            nt.links.new(bp.outputs['Normal'], b.inputs['Normal'])
        return m

    def dotted(self, m, pitch, dot, ink):
        nt = m.node_tree
        b = nt.nodes['Principled BSDF']
        tc = nt.nodes.new('ShaderNodeTexCoord')
        mp = nt.nodes.new('ShaderNodeMapping')
        mp.inputs['Scale'].default_value = (1 / pitch, 1 / pitch, 1)
        nt.links.new(tc.outputs['Object'], mp.inputs['Vector'])
        fr = nt.nodes.new('ShaderNodeVectorMath'); fr.operation = 'FRACTION'
        nt.links.new(mp.outputs['Vector'], fr.inputs[0])
        sub = nt.nodes.new('ShaderNodeVectorMath'); sub.operation = 'SUBTRACT'; sub.inputs[1].default_value = (0.5, 0.5, 0)
        nt.links.new(fr.outputs[0], sub.inputs[0])
        sep = nt.nodes.new('ShaderNodeSeparateXYZ'); nt.links.new(sub.outputs[0], sep.inputs[0])
        comb = nt.nodes.new('ShaderNodeCombineXYZ')
        nt.links.new(sep.outputs['X'], comb.inputs['X']); nt.links.new(sep.outputs['Y'], comb.inputs['Y'])
        ln = nt.nodes.new('ShaderNodeVectorMath'); ln.operation = 'LENGTH'
        nt.links.new(comb.outputs[0], ln.inputs[0])
        st = nt.nodes.new('ShaderNodeMath'); st.operation = 'LESS_THAN'; st.inputs[1].default_value = dot
        nt.links.new(ln.outputs['Value'], st.inputs[0])
        mx = nt.nodes.new('ShaderNodeMix'); mx.data_type = 'RGBA'
        mx.inputs[6].default_value = tuple(b.inputs['Base Color'].default_value)
        mx.inputs[7].default_value = (*ink, 1)
        nt.links.new(st.outputs[0], mx.inputs[0])
        nt.links.new(mx.outputs[2], b.inputs['Base Color'])
        return m

    def image_mat(self, name, path, rough=0.35, coat=0.6, emission_only=False, value=1.0):
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        nt = m.node_tree
        t = nt.nodes.new('ShaderNodeTexImage')
        t.image = bpy.data.images.load(path)
        t.extension = 'CLIP'
        col = t.outputs['Color']
        if value != 1.0:
            hsv = nt.nodes.new('ShaderNodeHueSaturation'); hsv.inputs['Value'].default_value = value
            nt.links.new(col, hsv.inputs['Color']); col = hsv.outputs['Color']
        if emission_only:
            for n in list(nt.nodes):
                if n.type == 'BSDF_PRINCIPLED':
                    nt.nodes.remove(n)
            e = nt.nodes.new('ShaderNodeEmission')
            nt.links.new(col, e.inputs['Color'])
            nt.links.new(e.outputs['Emission'], nt.nodes['Material Output'].inputs['Surface'])
            return m
        b = nt.nodes['Principled BSDF']
        nt.links.new(col, b.inputs['Base Color'])
        b.inputs['Roughness'].default_value = rough
        b.inputs['Coat Weight'].default_value = coat
        return m

    def lcd_mat(self, name='lcd', grid=0.06):
        T = self.T
        m = bpy.data.materials.new(name); m.use_nodes = True
        nt = m.node_tree; b = nt.nodes['Principled BSDF']
        tc = nt.nodes.new('ShaderNodeTexCoord')
        gr = nt.nodes.new('ShaderNodeTexGradient'); gr.gradient_type = 'SPHERICAL'
        mp = nt.nodes.new('ShaderNodeMapping'); mp.inputs['Scale'].default_value = (0.22, 0.5, 1); mp.inputs['Location'].default_value = (0.1, 0.25, 0)
        nt.links.new(tc.outputs['Object'], mp.inputs['Vector']); nt.links.new(mp.outputs['Vector'], gr.inputs['Vector'])
        ramp = nt.nodes.new('ShaderNodeValToRGB')
        ramp.color_ramp.elements[0].color = (*hexc(T['lcdB']), 1)
        ramp.color_ramp.elements[1].color = (*hexc(T['lcdA']), 1)
        nt.links.new(gr.outputs['Fac'], ramp.inputs['Fac'])
        mp2 = nt.nodes.new('ShaderNodeMapping'); mp2.inputs['Scale'].default_value = (1 / grid, 1 / grid, 1)
        nt.links.new(tc.outputs['Object'], mp2.inputs['Vector'])
        fr = nt.nodes.new('ShaderNodeVectorMath'); fr.operation = 'FRACTION'; nt.links.new(mp2.outputs[0], fr.inputs[0])
        sep = nt.nodes.new('ShaderNodeSeparateXYZ'); nt.links.new(fr.outputs[0], sep.inputs[0])
        lx = nt.nodes.new('ShaderNodeMath'); lx.operation = 'LESS_THAN'; lx.inputs[1].default_value = 0.12; nt.links.new(sep.outputs['X'], lx.inputs[0])
        ly = nt.nodes.new('ShaderNodeMath'); ly.operation = 'LESS_THAN'; ly.inputs[1].default_value = 0.12; nt.links.new(sep.outputs['Y'], ly.inputs[0])
        mxg = nt.nodes.new('ShaderNodeMath'); mxg.operation = 'MAXIMUM'; nt.links.new(lx.outputs[0], mxg.inputs[0]); nt.links.new(ly.outputs[0], mxg.inputs[1])
        gm = nt.nodes.new('ShaderNodeMath'); gm.operation = 'MULTIPLY'; gm.inputs[1].default_value = 0.07; nt.links.new(mxg.outputs[0], gm.inputs[0])
        mix = nt.nodes.new('ShaderNodeMix'); mix.data_type = 'RGBA'
        nt.links.new(gm.outputs[0], mix.inputs[0]); nt.links.new(ramp.outputs['Color'], mix.inputs[6]); mix.inputs[7].default_value = (*hexc('#262e1e'), 1)
        nt.links.new(mix.outputs[2], b.inputs['Base Color'])
        nt.links.new(mix.outputs[2], b.inputs['Emission Color'])
        b.inputs['Emission Strength'].default_value = T['lcdEmit']
        b.inputs['Roughness'].default_value = 0.55
        return m

    def _mats(self):
        T = self.T
        c = hexc
        self.CHALK = self.dotted(self.mat('chalk', c(T['chassis']), 0.8, bump=(520, 0.14), spec=0.2), 0.13, 0.012, c(T['dots']))
        self.TRAY = self.dotted(self.mat('tray', c(T['tray']), 0.82, bump=(520, 0.14), spec=0.18), 0.13, 0.012, c(T['trayDots']))
        self.GRAPH = self.dotted(self.mat('graphite', c(T['bay']), 0.5, bump=(1100, 0.08), spec=0.4), 0.08, 0.009, c(T['bayDots']))
        self.WELL = self.mat('well', c(T['well']), 0.35, bump=(1200, 0.03))
        self.KEY = self.mat('key', c(T['key']), 0.42, coat=0.18, bump=(1400, 0.035), spec=0.3)
        self.KEYS = self.mat('keys', c(T['keyStation']), 0.42, coat=0.3, bump=(1400, 0.04))
        self.INK = self.mat('ink', c(T['ink']), 0.6)
        self.INKD = self.mat('inkd', c(T['inkD']), 0.6)
        self.INKS = self.mat('inks', c(T['inkSoft']), 0.6)
        self.WHITE = self.mat('white', c('#eceef2'), 0.5)
        self.GREY = self.mat('grey', c('#8a909b'), 0.55)
        self.DARKINK = self.mat('darkink', c('#1f2329'), 0.6)
        self.ALU = self.mat('alu', c('#c9ccd2'), 0.22, metal=1.0, aniso=0.85)
        self.ALUD = self.mat('alud', c('#9aa0aa'), 0.35, metal=1.0)
        self.CHROME = self.mat('chrome', c('#e6e8ec'), 0.08, metal=1.0)
        self.RED = self.mat('red', c('#ff3b30'), 0.3, emit=c('#ff3b30'), estr=5)
        self.LEDG = self.mat('ledg', c('#3ee08f'), 0.3, emit=c('#3ee08f'), estr=5)
        self.LEDOFF = self.mat('ledoff', c('#b9bec7') if self.theme == 'light' else c('#3a3d44'), 0.3)
        self.GLASS = self.mat('glass', (1, 1, 1), 0.02, trans=1.0, ior=1.5, spec=0.6)
        self.LYR = self.mat('lyr', c('#d4ef86'), 0.4, emit=c('#c4e078'), estr=3.2)
        self.LYRD = self.mat('lyrd', c('#56603f'), 0.5, emit=c('#56603f'), estr=0.8)
        self.GOLD = self.mat('gold', c('#f5b93b'), 0.35, emit=c('#f5b93b'), estr=1.5)
        self.RIM = self.mat('rim', c(T['rim']), 0.35, coat=0.4)
        self.LCD = self.lcd_mat()
        self.LCDINK = self.mat('lcdink', c('#232b1b'), 0.6)
        self.LCDGHOST = self.mat('lcdghost', c('#9faa80') if self.theme == 'light' else c('#7f8b5e'), 0.6)
        self.LCDFRAME = self.mat('lcdframe', c(T['lcdFrame']), 0.5)
        self.PAPER = self.mat('paper', c(T['paper']), 0.75, bump=(260, 0.05), sss=0.05)
        self.THERMAL = self.mat('thermal', c('#3a3a3c'), 0.8)
        # real LEDs: a tinted dome lens, dark when off, glowing from inside when on
        self.LED_ON = self.mat('ledon', c('#ffcf8a'), 0.15, coat=1.0, emit=c('#ffa640'), estr=3.2, spec=0.8)
        self.LED_HOT = self.mat('ledhot', c('#ff9a7a'), 0.15, coat=1.0, emit=c('#ff4a2a'), estr=3.0, spec=0.8)
        self.LED_DIM = self.mat('leddim', c('#5a3512'), 0.12, coat=1.0, spec=0.9)
        self.LED_DIMR = self.mat('leddimr', c('#4a1510'), 0.12, coat=1.0, spec=0.9)
        self.BEZEL = self.mat('bezel', c('#16171a'), 0.4)

    # ── geometry
    def link(self, o):
        self.sc.collection.objects.link(o)
        return o

    def rbox(self, name, x0, y0, x1, y1, z0, z1, r, bev, m, segs=12):
        ax, ay = P(x0, y0); bx, by = P(x1, y1)
        bm = bmesh.new()
        bmesh.ops.create_cube(bm, size=1)
        for v in bm.verts:
            v.co.x = ax if v.co.x < 0 else bx
            v.co.y = by if v.co.y < 0 else ay
            v.co.z = z0 if v.co.z < 0 else z1
        if r > 0:
            ve = [e for e in bm.edges if abs(e.verts[0].co.x - e.verts[1].co.x) < 1e-6 and abs(e.verts[0].co.y - e.verts[1].co.y) < 1e-6]
            rr = min(r / 100, (x1 - x0) / 200 - 1e-4, (y1 - y0) / 200 - 1e-4)
            bmesh.ops.bevel(bm, geom=ve, offset=rr, offset_type='OFFSET', segments=segs, profile=0.5, affect='EDGES', clamp_overlap=True)
        me = bpy.data.meshes.new(name)
        bm.to_mesh(me); bm.free()
        for p in me.polygons:
            p.use_smooth = True
        o = self.link(bpy.data.objects.new(name, me))
        o.data.materials.append(m)
        if bev > 0:
            md = o.modifiers.new('bev', 'BEVEL')
            md.width = bev / 100
            md.segments = 6
            md.limit_method = 'ANGLE'
            md.angle_limit = math.radians(50)
            md.harden_normals = True
        return o

    def cyl(self, name, cx, cy, rad, z0, z1, m, bev=0, segs=96):
        bm = bmesh.new()
        bmesh.ops.create_cone(bm, cap_ends=True, segments=segs, radius1=rad / 100, radius2=rad / 100, depth=z1 - z0)
        me = bpy.data.meshes.new(name); bm.to_mesh(me); bm.free()
        for p in me.polygons:
            p.use_smooth = True
        o = self.link(bpy.data.objects.new(name, me))
        x, y = P(cx, cy)
        o.location = (x, y, (z0 + z1) / 2)
        o.data.materials.append(m)
        if bev:
            md = o.modifiers.new('bev', 'BEVEL'); md.width = bev / 100; md.segments = 6
            md.limit_method = 'ANGLE'; md.angle_limit = math.radians(50); md.harden_normals = True
        return o

    def dome(self, name, cx, cy, rad, z, m, h=0.6):
        """A half-sphere lens (LEDs)."""
        bm = bmesh.new()
        bmesh.ops.create_uvsphere(bm, u_segments=32, v_segments=16, radius=rad / 100)
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if v.co.z < -1e-6], context='VERTS')
        for v in bm.verts:
            v.co.z *= h
        me = bpy.data.meshes.new(name); bm.to_mesh(me); bm.free()
        for p in me.polygons:
            p.use_smooth = True
        o = self.link(bpy.data.objects.new(name, me))
        x, y = P(cx, cy)
        o.location = (x, y, z)
        o.data.materials.append(m)
        return o

    def cut(self, target, cutter, first=False):
        md = target.modifiers.new('cut_' + cutter.name, 'BOOLEAN')
        md.operation = 'DIFFERENCE'
        md.solver = 'EXACT'
        md.object = cutter
        cutter.hide_render = True
        if first:
            target.modifiers.move(len(target.modifiers) - 1, 0)
        return md

    def font(self, name):
        if name not in self._fonts:
            self._fonts[name] = bpy.data.fonts.load(os.path.join(FONTS, name))
        return self._fonts[name]

    SCALE = {'galmuri11.ttf': 1.45, 'dotgothic16.ttf': 1.1, 'pixelify_600.ttf': 1.2, 'michroma.ttf': 1.3, 'mplus_r_xb.ttf': 1.12, 'mplus_r_md.ttf': 1.05, 'mplus_r_bd.ttf': 1.05, 'dela.ttf': 1.0}

    def text(self, body, x, y, size_px, fnt, m, z, align='LEFT', valign='TOP_BASELINE', extrude=0.0, spacing=1.0, rot=0):
        cu = bpy.data.curves.new('t', 'FONT')
        cu.body = body
        cu.font = self.font(fnt)
        cu.size = size_px / 100 * self.SCALE.get(fnt, 1.0)
        cu.align_x = align
        cu.align_y = valign
        cu.extrude = extrude
        cu.space_character = spacing
        o = self.link(bpy.data.objects.new('t_' + body[:12], cu))
        xx, yy = P(x, y)
        o.location = (xx, yy, z)
        o.rotation_euler = (0, 0, math.radians(rot))
        o.data.materials.append(m)
        return o

    def plane(self, name, x0, y0, x1, y1, z, m, rot=0, r=0):
        o = self.rbox(name, x0, y0, x1, y1, z - 0.002, z, r, 0, m, segs=10)
        me = o.data
        me.uv_layers.new()
        ax, ay = P(x0, y0); bx, by = P(x1, y1)
        for loop in me.loops:
            co = me.vertices[loop.vertex_index].co
            me.uv_layers.active.data[loop.index].uv = ((co.x - ax) / (bx - ax), (co.y - by) / (ay - by))
        if rot:
            cx, cy = (ax + bx) / 2, (ay + by) / 2
            me.transform(Matrix.Translation((-cx, -cy, 0)))
            o.location = (cx, cy, 0)
            o.rotation_euler = (0, math.radians(rot), 0)
        return o

    def key(self, name, x0, y0, x1, y1, r, down=False, m=None, z=0.0):
        zt = z + (0.05 if down else 0.11)
        k = self.rbox(name, x0, y0, x1, y1, z - 0.05, zt, r, 3.2, m or self.KEY)
        return k, zt

    def icon(self, name, cx, cy, px, m, z, h=0.006):
        rows = ICONS[name]
        w, hh = len(rows[0]), len(rows)
        bm = bmesh.new()
        s = px / 100
        x0, y0 = P(cx, cy)
        x0 -= w * s / 2; y0 += hh * s / 2
        for j, row in enumerate(rows):
            for i, ch in enumerate(row):
                if ch != '#':
                    continue
                vs = [bm.verts.new((x0 + i * s + dx * s, y0 - j * s - dy * s, z)) for dx, dy in ((0, 0), (1, 0), (1, 1), (0, 1))]
                bm.faces.new(vs)
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-6)
        f = bmesh.ops.extrude_face_region(bm, geom=bm.faces[:])
        for v in [e for e in f['geom'] if isinstance(e, bmesh.types.BMVert)]:
            v.co.z += h
        me = bpy.data.meshes.new('ic'); bm.to_mesh(me); bm.free()
        o = self.link(bpy.data.objects.new('ic_' + name, me))
        o.data.materials.append(m)
        return o

    def bloom(self, strength=0.45, threshold=1.2, size=0.55):
        sc = self.sc
        sc.use_nodes = True
        tree = sc.node_tree
        rl = tree.nodes.get('Render Layers') or tree.nodes.new('CompositorNodeRLayers')
        comp = tree.nodes.get('Composite') or tree.nodes.new('CompositorNodeComposite')
        try:
            gl = tree.nodes.new('CompositorNodeGlare')
            try:
                gl.glare_type = 'BLOOM'
            except Exception:
                gl.glare_type = 'FOG_GLOW'
            for nm, v in (('Threshold', threshold), ('Strength', strength), ('Size', size)):
                if nm in gl.inputs:
                    try:
                        gl.inputs[nm].default_value = v
                    except Exception:
                        pass
            tree.links.new(rl.outputs['Image'], gl.inputs['Image'])
            tree.links.new(gl.outputs['Image'], comp.inputs['Image'])
        except Exception as e:
            print('bloom failed', e)
            tree.links.new(rl.outputs['Image'], comp.inputs['Image'])


ICONS = {
    'RADIO': [".........##.", ".......##...", ".....##.....", "############", "#..........#", "#.####..##.#", "#.####.#..##", "#.####.#..##", "#.####..##.#", "#..........#", "############"],
    'SEARCH': ["..#####.....", ".##...##....", "##.....##...", "#.......#...", "#.......#...", "#.......#...", "##.....##...", ".##...###...", "..#####.##..", ".........##.", "..........##", "...........#"],
    'CHAT': [".##########.", "############", "##........##", "##.##.##.###", "##........##", "############", ".##########.", "..###.......", "..##........", "..#........."],
    'LIBRARY': ["##.##..##...", "##.##..###..", "##.##...##..", "##.##...###.", "##.##....##.", "##.##....###", "##.##.....##", "##.##.....##", "############"],
    'USER': ["....####....", "...######...", "...######...", "...######...", "....####....", "............", "..########..", ".##########.", "############", "############"],
    'GEAR': [".....##.....", "..#..##..#..", ".##########.", "..########..", "..###..###..", "####....####", "####....####", "..###..###..", "..########..", ".##########.", "..#..##..#..", ".....##....."],
    'PLAY': ["##......", "####....", "######..", "########", "########", "######..", "####....", "##......"],
    'PAUSE': ["###..###", "###..###", "###..###", "###..###", "###..###", "###..###", "###..###", "###..###"],
    'HEART': [".###...###.", "#...#.#...#", "#....#....#", "#.........#", ".#.......#.", "..#.....#..", "...#...#...", "....#.#....", ".....#....."],
    'HEART_FULL': [".###...###.", "###########", "###########", "###########", ".#########.", "..#######..", "...#####...", "....###....", ".....#....."],
    'INFO': [".##########.", "#..........#", "#....##....#", "#..........#", "#...###....#", "#....##....#", "#....##....#", "#....##....#", "#...####...#", "#..........#", ".##########."],
    'SAVE': ["##########..", "#..#####.##.", "#..#####..##", "#..##.##...#", "#..#####...#", "#..........#", "#.########.#", "#.#......#.#", "#.########.#", "#.#......#.#", "############"],
    'NOTE': ["...#######", "...#######", "...#.....#", "...#.....#", "...#.....#", "...#.....#", ".###...###", "####..####", "####..####", ".##....##."],
    'CHECK': [".........##", "........##.", ".......##..", "##....##...", ".##..##....", "..####.....", "...##......"],
    'STAR': [".....##.....", ".....##.....", "....####....", "############", ".##########.", "..########..", "...######...", "..###..###..", ".##......##."],
    'NEXT': ["#....#.#", "##...#.#", "###..#.#", "####.#.#", "####.#.#", "###..#.#", "##...#.#", "#....#.#"],
    'PREV': ["#.#....#", "#.#...##", "#.#..###", "#.#.####", "#.#.####", "#.#..###", "#.#...##", "#.#....#"],
}
