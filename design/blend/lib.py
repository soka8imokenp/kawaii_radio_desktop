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
        import sys as _s
        _v = _s.argv[_s.argv.index('--view') + 1] if '--view' in _s.argv else 'std'
        if _v == 'agx':
            sc.view_settings.view_transform = 'AgX'
            sc.view_settings.look = 'AgX - Medium High Contrast'
        else:
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
    def area(self, name, loc, size, energy, color=(1, 1, 1), target=(0, 0, 0), camera=False, glossy=True, shape='SQUARE', size_y=None, spread=None):
        d = bpy.data.lights.new(name, 'AREA')
        if spread:
            d.spread = math.radians(spread)   # a gridded softbox: light goes where it points, no broad wash
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
        old = '--oldlight' in __import__('sys').argv
        # a gridded key: a narrower spread concentrates the same energy, so it is scaled down to keep the exposure
        self.area('key', (-9, 9, 11), 9, T['key_light'] * (1 if old else 0.42), (1.0, 0.98, 0.95), spread=None if old else 50)
        self.area('fill', (10, -6, 9), 12, T['fill'], (0.92, 0.95, 1.0))
        self.area('top', (0, 0, 14), 18, T['top'])
        import sys as _s
        if '--flat' not in _s.argv:
            # product-shot light: long strip softboxes grazing the device, so every chamfer and edge catches a line of light
            dk = self.theme == 'dark'
            old = '--oldlight' in _s.argv
            sp = None if old else 40
            self.area('rimN', (0, 9.5, 2.2), 26, (300 if dk else 100) * (1 if old else 0.55), (1.0, 0.97, 0.93), target=(0, 0, 0), shape='RECTANGLE', size_y=0.6, spread=sp)
            self.area('rimW', (-14, 0, 2.6), 14, (560 if dk else 110) * (1 if old else 0.55), (0.72, 0.84, 1.0) if dk else (0.9, 0.94, 1.0), target=(0, 0, 0), shape='RECTANGLE', size_y=0.5, spread=sp)
            if old:
                self.area('grad', (-3, 6, 10), 9, 420 if dk else 110, (1.0, 0.97, 0.93), target=(0, -1, 0))
            else:
                # no broad light from above (it washed the upper left): two gridded strips graze from the front-left
                # and the bottom-right, so chamfers and key edges get lines of light and the flats stay deep
                self.area('grazeFL', (-12, -7.5, 2.4), 16, 200 if dk else 70, (1.0, 0.97, 0.93), target=(-1, 0, 0), shape='RECTANGLE', size_y=0.6, spread=40)
                self.area('grazeBR', (12, -8, 2.2), 14, 140 if dk else 50, (0.85, 0.9, 1.0), target=(1, 0, 0), shape='RECTANGLE', size_y=0.6, spread=40)

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

    def led(self, name, core, edge, estr):
        """A real diffused LED: a hot, almost white die in the middle, the saturated colour toward the lens rim."""
        m = self.mat(name, edge, 0.35, coat=1.0, spec=0.8)
        nt = m.node_tree; b = nt.nodes['Principled BSDF']
        lw = nt.nodes.new('ShaderNodeLayerWeight'); lw.inputs['Blend'].default_value = 0.35
        ramp = nt.nodes.new('ShaderNodeValToRGB')
        ramp.color_ramp.elements[0].color = (*core, 1)
        ramp.color_ramp.elements[1].position = 0.55
        ramp.color_ramp.elements[1].color = (*edge, 1)
        nt.links.new(lw.outputs['Facing'], ramp.inputs['Fac'])
        nt.links.new(ramp.outputs['Color'], b.inputs['Emission Color'])
        mr = nt.nodes.new('ShaderNodeMapRange')
        mr.inputs['To Min'].default_value = estr; mr.inputs['To Max'].default_value = estr * 0.25
        nt.links.new(lw.outputs['Facing'], mr.inputs['Value'])
        nt.links.new(mr.outputs['Result'], b.inputs['Emission Strength'])
        return m

    def smudge(self, m, scale=2.2, amount=0.12, fine=0.0):
        """Real surfaces are not uniform: low-frequency roughness variation (handling, wiping) and
        an optional fine bead-blast grain in the normal, both only visible in reflections."""
        nt = m.node_tree
        b = nt.nodes['Principled BSDF']
        tc = nt.nodes.new('ShaderNodeTexCoord')
        n = nt.nodes.new('ShaderNodeTexNoise'); n.inputs['Scale'].default_value = scale; n.inputs['Detail'].default_value = 6
        n.inputs['Roughness'].default_value = 0.65
        nt.links.new(tc.outputs['Object'], n.inputs['Vector'])
        sub = nt.nodes.new('ShaderNodeMath'); sub.operation = 'SUBTRACT'; sub.inputs[1].default_value = 0.5
        nt.links.new(n.outputs['Fac'], sub.inputs[0])
        mul = nt.nodes.new('ShaderNodeMath'); mul.operation = 'MULTIPLY'; mul.inputs[1].default_value = amount * 2
        nt.links.new(sub.outputs[0], mul.inputs[0])
        add = nt.nodes.new('ShaderNodeMath'); add.operation = 'ADD'; add.use_clamp = True
        rin = b.inputs['Roughness']
        if rin.is_linked:
            src = rin.links[0].from_socket
            nt.links.new(src, add.inputs[0])
        else:
            add.inputs[0].default_value = rin.default_value
        nt.links.new(mul.outputs[0], add.inputs[1])
        nt.links.new(add.outputs[0], rin)
        if fine:
            g = nt.nodes.new('ShaderNodeTexNoise'); g.inputs['Scale'].default_value = 2600; g.inputs['Detail'].default_value = 2
            nt.links.new(tc.outputs['Object'], g.inputs['Vector'])
            bp = nt.nodes.new('ShaderNodeBump'); bp.inputs['Strength'].default_value = fine; bp.inputs['Distance'].default_value = 0.004
            nt.links.new(g.outputs['Fac'], bp.inputs['Height'])
            nin = b.inputs['Normal']
            if nin.is_linked:
                nt.links.new(nin.links[0].from_socket, bp.inputs['Normal'])
            nt.links.new(bp.outputs['Normal'], nin)
        return m

    def pbr(self, m, asset, tile=6.0, rough_amt=0.0, normal=0.0, rough_map='Roughness'):
        """Scanned CC0 surface (ambientCG, design/blend/pbr/<asset>): its roughness map shifts the material's
        roughness by +-rough_amt and its normal map adds the real grain. Planar mapping, the camera looks straight down."""
        d = os.path.join(HERE, 'pbr', asset)
        if not os.path.isdir(d):
            return m
        def find(kind):
            for f in os.listdir(d):
                if f.endswith('_' + kind + '.jpg'):
                    return os.path.join(d, f)
        nt = m.node_tree
        b = nt.nodes['Principled BSDF']
        tc = nt.nodes.new('ShaderNodeTexCoord')
        mp = nt.nodes.new('ShaderNodeMapping'); mp.inputs['Scale'].default_value = (1 / tile, 1 / tile, 1 / tile)
        nt.links.new(tc.outputs['Object'], mp.inputs['Vector'])
        def img(path):
            t = nt.nodes.new('ShaderNodeTexImage'); t.image = bpy.data.images.load(path, check_existing=True)
            t.image.colorspace_settings.name = 'Non-Color'; t.extension = 'REPEAT'; t.interpolation = 'Cubic'
            nt.links.new(mp.outputs['Vector'], t.inputs['Vector'])
            return t
        rp = find(rough_map)
        if rough_amt and rp:
            t = img(rp)
            sub = nt.nodes.new('ShaderNodeMath'); sub.operation = 'SUBTRACT'; sub.inputs[1].default_value = 0.5
            nt.links.new(t.outputs['Color'], sub.inputs[0])
            mul = nt.nodes.new('ShaderNodeMath'); mul.operation = 'MULTIPLY'; mul.inputs[1].default_value = rough_amt * 2
            nt.links.new(sub.outputs[0], mul.inputs[0])
            add = nt.nodes.new('ShaderNodeMath'); add.operation = 'ADD'; add.use_clamp = True
            rin = b.inputs['Roughness']
            if rin.is_linked:
                nt.links.new(rin.links[0].from_socket, add.inputs[0])
            else:
                add.inputs[0].default_value = rin.default_value
            nt.links.new(mul.outputs[0], add.inputs[1]); nt.links.new(add.outputs[0], rin)
        np_ = find('NormalGL')
        if normal and np_:
            t = img(np_)
            nm = nt.nodes.new('ShaderNodeNormalMap'); nm.inputs['Strength'].default_value = normal
            nt.links.new(t.outputs['Color'], nm.inputs['Color'])
            # feed it under the existing bump chain, so procedural and scanned detail add up
            sock = b.inputs['Normal']
            while sock.is_linked:
                n = sock.links[0].from_node
                if 'Normal' in n.inputs:
                    sock = n.inputs['Normal']
                else:
                    break
            if not sock.is_linked:
                nt.links.new(nm.outputs['Normal'], sock)
        return m

    def plastic(self, m, rough_spread=0.12, bump=0.08, grain=90.0, dome=0.0):
        """Procedural plastic (the SILVER PUNK recipe, scaled to the device):
        roughness = noise -> ramp (0.3..0.8) around the base value, so the highlight is never uniform;
        bump = fine noise x multifractal noise (multiply) -> inverted Bump: moulded micro-texture;
        dome > 0 bends the normal of flat tops into a slight convex cap, so keys catch a rolling highlight."""
        nt = m.node_tree
        b = nt.nodes['Principled BSDF']
        tc = nt.nodes.new('ShaderNodeTexCoord')
        # roughness breakup
        n = nt.nodes.new('ShaderNodeTexNoise'); n.inputs['Scale'].default_value = grain * 0.18
        n.inputs['Detail'].default_value = 10; n.inputs['Roughness'].default_value = 1.0; n.inputs['Distortion'].default_value = 0.1
        nt.links.new(tc.outputs['Object'], n.inputs['Vector'])
        ramp = nt.nodes.new('ShaderNodeValToRGB')
        ramp.color_ramp.elements[0].position = 0.3; ramp.color_ramp.elements[1].position = 0.8
        nt.links.new(n.outputs['Fac'], ramp.inputs['Fac'])
        mr = nt.nodes.new('ShaderNodeMapRange')
        rin = b.inputs['Roughness']
        base = rin.default_value
        mr.inputs['To Min'].default_value = max(0.05, base - rough_spread); mr.inputs['To Max'].default_value = min(1.0, base + rough_spread)
        nt.links.new(ramp.outputs['Color'], mr.inputs['Value'])
        if rin.is_linked:   # keep earlier variation (smudges, scans): average them
            avg = nt.nodes.new('ShaderNodeMath'); avg.operation = 'ADD'
            mh = nt.nodes.new('ShaderNodeMath'); mh.operation = 'MULTIPLY'; mh.inputs[1].default_value = 0.5
            nt.links.new(rin.links[0].from_socket, avg.inputs[0]); nt.links.new(mr.outputs['Result'], avg.inputs[1])
            nt.links.new(avg.outputs[0], mh.inputs[0]); nt.links.new(mh.outputs[0], rin)
        else:
            nt.links.new(mr.outputs['Result'], rin)
        # dense polymer: a trace of subsurface light, plastic IOR
        b.inputs['Subsurface Weight'].default_value = 0.03
        b.inputs['Subsurface Radius'].default_value = (0.1, 0.1, 0.1)
        if 'Subsurface Scale' in b.inputs:
            b.inputs['Subsurface Scale'].default_value = 0.01
        b.inputs['IOR'].default_value = 1.48
        # crevices are a little rougher (dust, less polish): ambient occlusion -> crushed ramp -> + roughness
        ao = nt.nodes.new('ShaderNodeAmbientOcclusion'); ao.inputs['Distance'].default_value = 0.08; ao.samples = 8
        aor = nt.nodes.new('ShaderNodeValToRGB')
        aor.color_ramp.elements[0].position = 0.35; aor.color_ramp.elements[0].color = (0.12, 0.12, 0.12, 1)
        aor.color_ramp.elements[1].position = 0.9; aor.color_ramp.elements[1].color = (0, 0, 0, 1)
        nt.links.new(ao.outputs['AO'], aor.inputs['Fac'])
        rsum = nt.nodes.new('ShaderNodeMath'); rsum.operation = 'ADD'; rsum.use_clamp = True
        nt.links.new(rin.links[0].from_socket, rsum.inputs[0]); nt.links.new(aor.outputs['Color'], rsum.inputs[1])
        nt.links.new(rsum.outputs[0], rin)
        # moulded micro relief: two noises multiplied
        a = nt.nodes.new('ShaderNodeTexNoise'); a.inputs['Scale'].default_value = grain
        a.inputs['Detail'].default_value = 8; a.inputs['Roughness'].default_value = 0.9; a.inputs['Distortion'].default_value = 0.1
        c = nt.nodes.new('ShaderNodeTexNoise')
        try:
            c.noise_type = 'MULTIFRACTAL'
        except Exception:
            pass
        c.inputs['Scale'].default_value = grain * 1.5; c.inputs['Detail'].default_value = 10
        c.inputs['Roughness'].default_value = 1.0; c.inputs['Distortion'].default_value = 2.0
        for t in (a, c):
            nt.links.new(tc.outputs['Object'], t.inputs['Vector'])
        mx = nt.nodes.new('ShaderNodeMix'); mx.data_type = 'RGBA'; mx.blend_type = 'MULTIPLY'; mx.inputs['Factor'].default_value = 1.0
        nt.links.new(a.outputs['Fac'], mx.inputs[6]); nt.links.new(c.outputs['Fac'], mx.inputs[7])
        bp = nt.nodes.new('ShaderNodeBump'); bp.invert = True
        bp.inputs['Strength'].default_value = bump; bp.inputs['Distance'].default_value = 0.003
        nt.links.new(mx.outputs[2], bp.inputs['Height'])
        if dome:
            # convex cap: on faces looking up, tilt the normal by the position inside the object's bounds
            g = nt.nodes.new('ShaderNodeNewGeometry')
            sub = nt.nodes.new('ShaderNodeVectorMath'); sub.operation = 'SUBTRACT'; sub.inputs[1].default_value = (0.5, 0.5, 0.5)
            nt.links.new(tc.outputs['Generated'], sub.inputs[0])
            sc_ = nt.nodes.new('ShaderNodeVectorMath'); sc_.operation = 'MULTIPLY'; sc_.inputs[1].default_value = (dome, dome, 0)
            nt.links.new(sub.outputs[0], sc_.inputs[0])
            add = nt.nodes.new('ShaderNodeVectorMath'); add.operation = 'ADD'
            nt.links.new(sc_.outputs[0], add.inputs[0]); nt.links.new(g.outputs['Normal'], add.inputs[1])
            nrm = nt.nodes.new('ShaderNodeVectorMath'); nrm.operation = 'NORMALIZE'
            nt.links.new(add.outputs[0], nrm.inputs[0])
            sep = nt.nodes.new('ShaderNodeSeparateXYZ'); nt.links.new(g.outputs['Normal'], sep.inputs[0])
            up = nt.nodes.new('ShaderNodeMath'); up.operation = 'GREATER_THAN'; up.inputs[1].default_value = 0.985
            nt.links.new(sep.outputs['Z'], up.inputs[0])
            mv = nt.nodes.new('ShaderNodeMix'); mv.data_type = 'VECTOR'
            nt.links.new(up.outputs[0], mv.inputs['Factor']); nt.links.new(g.outputs['Normal'], mv.inputs[4]); nt.links.new(nrm.outputs[0], mv.inputs[5])
            nt.links.new(mv.outputs[1], bp.inputs['Normal'])
        # put it under the existing normal chain (procedural bumps stack; a normal map consumes the bottom)
        nin = b.inputs['Normal']
        if nin.is_linked:
            top = nin.links[0].from_node
            if top.type == 'BUMP' and not top.inputs['Normal'].is_linked:
                nt.links.new(bp.outputs['Normal'], top.inputs['Normal'])
            else:
                nt.links.new(bp.outputs['Normal'], nin)   # replace (keeps the result simple and predictable)
        else:
            nt.links.new(bp.outputs['Normal'], nin)
        return m

    def flake(self, m, scale=2600, amount=0.18, metal=0.0):
        """Sparkle in the plastic: tiny cells with their own roughness (and a little metal), like painted hardware."""
        nt = m.node_tree
        b = nt.nodes['Principled BSDF']
        tc = nt.nodes.new('ShaderNodeTexCoord')
        v = nt.nodes.new('ShaderNodeTexVoronoi'); v.inputs['Scale'].default_value = scale
        nt.links.new(tc.outputs['Object'], v.inputs['Vector'])
        mr = nt.nodes.new('ShaderNodeMapRange')
        mr.inputs['To Min'].default_value = b.inputs['Roughness'].default_value - amount
        mr.inputs['To Max'].default_value = b.inputs['Roughness'].default_value + amount * 0.3
        nt.links.new(v.outputs['Color'] if 'Color' in v.outputs else v.outputs[1], mr.inputs['Value'])
        nt.links.new(mr.outputs['Result'], b.inputs['Roughness'])
        if metal:
            mm = nt.nodes.new('ShaderNodeMath'); mm.operation = 'MULTIPLY'; mm.inputs[1].default_value = metal
            sep = nt.nodes.new('ShaderNodeSeparateColor')
            nt.links.new(v.outputs['Color'], sep.inputs['Color'])
            nt.links.new(sep.outputs[0], mm.inputs[0])
            nt.links.new(mm.outputs[0], b.inputs['Metallic'])
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
        self.CHALK = self.flake(self.dotted(self.mat('chalk', c(T['chassis']), 0.55, bump=(420, 0.2), spec=0.22), 0.13, 0.012, c(T['dots'])), 2400, 0.14, 0.0)
        self.TRAY = self.flake(self.dotted(self.mat('tray', c(T['tray']), 0.82, bump=(420, 0.2), spec=0.18), 0.13, 0.012, c(T['trayDots'])), 2400, 0.12, 0.0)
        self.GRAPH = self.flake(self.dotted(self.mat('graphite', c(T['bay']), 0.5, bump=(900, 0.1), spec=0.4), 0.08, 0.009, c(T['bayDots'])), 3000, 0.2, 0.0)
        self.WELL = self.mat('well', c(T['well']), 0.35, bump=(1200, 0.03))
        self.KEY = self.flake(self.mat('key', c(T['key']), 0.42, coat=0.18, bump=(1400, 0.035), spec=0.3), 3200, 0.1, 0.0)   # plastics are dielectric: metallic strictly 0, or they read as cheap plastic-metal
        import sys as _s
        if '--flat' not in _s.argv:
            for m_, sc_, am_, fi_ in ((self.CHALK, 1.6, 0.14, 0.5), (self.TRAY, 1.9, 0.12, 0.4), (self.GRAPH, 2.4, 0.16, 0.3), (self.KEY, 3.0, 0.08, 0.28)):
                self.smudge(m_, sc_, am_, fi_)
            if '--nopbr' not in _s.argv:
                self.pbr(self.CHALK, 'Plastic013A', 5.0, 0.10, 0.0)
                self.pbr(self.TRAY, 'Plastic013A', 5.5, 0.08, 0.0)
                self.pbr(self.GRAPH, 'Plastic006', 7.0, 0.10, 0.0)
                self.pbr(self.KEY, 'Plastic010', 4.0, 0.08, 0.0)
                self.pbr(self.KEY, 'Fingerprints002', 9.0, 0.10, 0.0)
            if '--noplastic' not in _s.argv:
                self.plastic(self.CHALK, 0.075, 0.10, 90)
                self.plastic(self.TRAY, 0.10, 0.08, 90)
                self.plastic(self.GRAPH, 0.12, 0.06, 110)
                self.plastic(self.KEY, 0.10, 0.07, 120, dome=0.35)
        self.KEYS = self.mat('keys', c(T['keyStation']), 0.42, coat=0.3, bump=(1400, 0.04))
        if '--flat' not in _s.argv and '--noplastic' not in _s.argv:
            self.plastic(self.KEYS, 0.10, 0.07, 120, dome=0.35)
        # brushed steel for trims: a dielectric-free metal (metallic 1), anisotropic, a little rough
        self.STEEL = self.brushed('steel', (0.56, 0.57, 0.58), 0.2, 0.34)
        self.STEELB = self.brushed('steelb', (0.66, 0.67, 0.69), 0.14, 0.3)
        self.INK = self.mat('ink', c(T['ink']), 0.6)
        self.INKD = self.mat('inkd', c(T['inkD']), 0.6)
        self.INKS = self.mat('inks', c(T['inkSoft']), 0.6)
        self.WHITE = self.mat('white', c('#eceef2'), 0.5)
        self.GREY = self.mat('grey', c('#8a909b'), 0.55)
        self.DARKINK = self.mat('darkink', c('#1f2329'), 0.6)
        self.ALU = self.brushed('alu', c('#c9ccd2'), 0.16, 0.28, kind='alu', aniso=0.7)
        self.ALUD = self.mat('alud', c('#9aa0aa'), 0.35, metal=1.0)
        self.CHROME = self.mat('chrome', c('#e6e8ec'), 0.08, metal=1.0)
        self.RED = self.mat('red', c('#ff3b30'), 0.3, emit=c('#ff3b30'), estr=5)
        self.LEDG = self.mat('ledg', c('#3ee08f'), 0.3, emit=c('#3ee08f'), estr=5)
        self.LEDOFF = self.mat('ledoff', c('#b9bec7') if self.theme == 'light' else c('#3a3d44'), 0.3)
        self.GLASS = self.mat('glass', (1, 1, 1), 0.04, trans=1.0, ior=1.52, spec=0.6)
        gb = self.GLASS.node_tree.nodes['Principled BSDF']
        gb.inputs['Base Color'].default_value = (*c('#f0fff8'), 1)
        if 'Thin Film Thickness' in gb.inputs:
            gb.inputs['Thin Film Thickness'].default_value = 280.0
            gb.inputs['Thin Film IOR'].default_value = 1.38
        import sys as _s
        if '--flat' not in _s.argv and '--nopbr' not in _s.argv:
            self.pbr(self.GLASS, 'Fingerprints002', 8.0, 0.07, 0.0)   # faint prints and wipes, seen only in the reflections
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
        self.LED_ON = self.led('ledon', c('#fff1d6'), c('#ff9a2e'), 2.2)
        self.LED_HOT = self.led('ledhot', c('#ffd8c8'), c('#ff3a1e'), 2.0)
        self.LED_DIM = self.mat('leddim', c('#2e1d12') if self.theme == 'dark' else c('#4a3426'), 0.18, coat=1.0, spec=0.7)   # smoked lens, dark until lit
        self.LED_GRN = self.led('ledgrn', c('#eafff0'), c('#2fd36b'), 2.0)
        self.LED_DIMG = self.mat('leddimg', c('#0f2016') if self.theme == 'dark' else c('#2c4636'), 0.18, coat=1.0, spec=0.7)
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
        try:
            me.set_sharp_from_angle(angle=math.radians(30))   # flat faces stay flat: no smeared diagonal shading on big panels
        except Exception:
            pass
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
        try:
            me.set_sharp_from_angle(angle=math.radians(30))
        except Exception:
            pass
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

    # measured complex IOR (n, k) for RGB: a physical conductor reads as real metal at every angle
    CONDUCTOR = {'steel': ((2.91, 2.95, 2.58), (3.08, 2.93, 2.61)), 'alu': ((1.35, 0.97, 0.62), (7.47, 6.40, 5.30))}

    def _metal_out(self, m, kind, rough, normal, aniso=0.0, tangent=None):
        """Replace the Principled BSDF with a Metallic BSDF (physical conductor, Blender 4.3+); Principled if missing."""
        nt = m.node_tree; L = nt.links.new
        pb = nt.nodes['Principled BSDF']
        try:
            mb = nt.nodes.new('ShaderNodeBsdfMetallic')
        except Exception:
            L(rough, pb.inputs['Roughness']); L(normal, pb.inputs['Normal'])
            return m
        mb.fresnel_type = 'PHYSICAL_CONDUCTOR'
        n, k = self.CONDUCTOR[kind]
        mb.inputs['IOR'].default_value = n
        mb.inputs['Extinction'].default_value = k
        mb.inputs['Anisotropy'].default_value = aniso
        L(rough, mb.inputs['Roughness']); L(normal, mb.inputs['Normal'])
        if tangent is not None:
            L(tangent, mb.inputs['Tangent'])
        L(mb.outputs['BSDF'], nt.nodes['Material Output'].inputs['Surface'])
        nt.nodes.remove(pb)
        return m

    def brushed(self, name, color, r0, r1, scale=1.0, kind='steel', aniso=0.55, bump=0.08):
        """Brushed metal. Graph: Object coords -> Mapping(scale) -> two noises stretched 600:1 (streaks along both axes,
        so a frame reads brushed on every side) -> Maximum -> Linear Light with a low-frequency noise (soft wipes,
        uneven polish) -> Map Range(r0..r1) -> Roughness; streaks + micro pits -> Bump -> Normal; physical conductor."""
        m = self.mat(name, color, (r0 + r1) / 2, metal=1.0)
        nt = m.node_tree; L = nt.links.new
        tc = nt.nodes.new('ShaderNodeTexCoord')
        mp = nt.nodes.new('ShaderNodeMapping'); mp.inputs['Scale'].default_value = (2.0 * scale,) * 3
        L(tc.outputs['Object'], mp.inputs['Vector'])
        streak = []
        for sc_ in ((1.0, 600.0, 1.0), (600.0, 1.0, 1.0)):
            m2 = nt.nodes.new('ShaderNodeMapping'); m2.inputs['Scale'].default_value = sc_
            L(mp.outputs['Vector'], m2.inputs['Vector'])
            n = nt.nodes.new('ShaderNodeTexNoise'); n.inputs['Scale'].default_value = 4.0; n.inputs['Detail'].default_value = 8
            n.inputs['Roughness'].default_value = 0.65
            L(m2.outputs['Vector'], n.inputs['Vector']); streak.append(n)
        mx = nt.nodes.new('ShaderNodeMath'); mx.operation = 'MAXIMUM'
        L(streak[0].outputs['Fac'], mx.inputs[0]); L(streak[1].outputs['Fac'], mx.inputs[1])
        wipe = nt.nodes.new('ShaderNodeTexNoise'); wipe.inputs['Scale'].default_value = 1.3; wipe.inputs['Detail'].default_value = 3
        L(mp.outputs['Vector'], wipe.inputs['Vector'])
        ll = nt.nodes.new('ShaderNodeMix'); ll.data_type = 'RGBA'; ll.blend_type = 'LINEAR_LIGHT'; ll.inputs['Factor'].default_value = 0.35
        L(mx.outputs['Value'], ll.inputs[6]); L(wipe.outputs['Color'], ll.inputs[7])
        bw = nt.nodes.new('ShaderNodeRGBToBW'); L(ll.outputs[2], bw.inputs['Color'])
        rr = nt.nodes.new('ShaderNodeMapRange'); rr.inputs['From Min'].default_value = 0.3; rr.inputs['From Max'].default_value = 0.85
        rr.inputs['To Min'].default_value = r0; rr.inputs['To Max'].default_value = r1
        L(bw.outputs['Val'], rr.inputs['Value'])
        pits = nt.nodes.new('ShaderNodeTexNoise'); pits.inputs['Scale'].default_value = 1800; pits.inputs['Detail'].default_value = 2
        L(mp.outputs['Vector'], pits.inputs['Vector'])
        b1 = nt.nodes.new('ShaderNodeBump'); b1.inputs['Strength'].default_value = bump; b1.inputs['Distance'].default_value = 0.002
        L(mx.outputs['Value'], b1.inputs['Height'])
        b2 = nt.nodes.new('ShaderNodeBump'); b2.inputs['Strength'].default_value = 0.04; b2.inputs['Distance'].default_value = 0.0005
        L(pits.outputs['Fac'], b2.inputs['Height']); L(b1.outputs['Normal'], b2.inputs['Normal'])
        return self._metal_out(m, kind, rr.outputs['Result'], b2.outputs['Normal'], aniso)

    def spun(self, name, color, r0=0.14, r1=0.28):
        """Spun (lathe-turned) aluminium, like the bottom of a pan: concentric rings (Wave, rings, spherical, a little
        distortion) in roughness and bump, and anisotropy along circular tangents around the knob axis."""
        m = self.mat(name, color, (r0 + r1) / 2, metal=1.0)
        nt = m.node_tree; L = nt.links.new
        tc = nt.nodes.new('ShaderNodeTexCoord')
        w = nt.nodes.new('ShaderNodeTexWave'); w.wave_type = 'RINGS'; w.rings_direction = 'SPHERICAL'
        w.inputs['Scale'].default_value = 260; w.inputs['Distortion'].default_value = 1.2; w.inputs['Detail'].default_value = 4
        w.inputs['Detail Roughness'].default_value = 0.7
        L(tc.outputs['Object'], w.inputs['Vector'])
        rr = nt.nodes.new('ShaderNodeMapRange'); rr.inputs['To Min'].default_value = r0; rr.inputs['To Max'].default_value = r1
        L(w.outputs['Fac'], rr.inputs['Value'])
        bp = nt.nodes.new('ShaderNodeBump'); bp.inputs['Strength'].default_value = 0.1; bp.inputs['Distance'].default_value = 0.001
        L(w.outputs['Fac'], bp.inputs['Height'])
        tg = nt.nodes.new('ShaderNodeTangent'); tg.direction_type = 'RADIAL'; tg.axis = 'Z'
        return self._metal_out(m, 'alu', rr.outputs['Result'], bp.outputs['Normal'], 0.75, tg.outputs['Tangent'])

    def knurl_z(self, name, cx, cy, R, R2, z0, z1, m, n=180):
        """A vertical knurled collar: a star profile (alternating radii) extruded upward, so each ridge catches light."""
        bm = bmesh.new()
        a0, a1 = [], []
        for i in range(n):
            a = 2 * math.pi * i / n
            r = (R if i % 2 == 0 else R2) / 100
            a0.append(bm.verts.new((r * math.cos(a), r * math.sin(a), z0)))
            a1.append(bm.verts.new((r * math.cos(a), r * math.sin(a), z1)))
        for i in range(n):
            j = (i + 1) % n
            bm.faces.new((a0[i], a0[j], a1[j], a1[i]))
        bm.faces.new(list(reversed(a0))); bm.faces.new(a1)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
        me = bpy.data.meshes.new(name); bm.to_mesh(me); bm.free()
        o = self.link(bpy.data.objects.new(name, me)); o.data.materials.append(m)
        o.location = (*P(cx, cy), 0)
        return o

    def trim(self, name, x0, y0, x1, y1, r):
        """A machined bezel around an opening (x0..y1 is the opening): a dark gap line, then a 6 px brushed-steel
        frame with a rounded, polished top edge that catches the rim lights, standing a little proud of the chassis."""
        g = 1.5
        gap = self.rbox(name + '_gap', x0 - g, y0 - g, x1 + g, y1 + g, -0.05, 0.003, r + g, 0, self.BEZEL)
        self.cut(gap, self.rbox(name + '_gc', x0, y0, x1, y1, -0.2, 0.2, r, 0, self.BEZEL))
        w = 3.5
        o = self.rbox(name, x0 - g - w, y0 - g - w, x1 + g + w, y1 + g + w, -0.01, 0.02, r + g + w, 0, self.STEELB)
        self.cut(o, self.rbox(name + '_c', x0 - g, y0 - g, x1 + g, y1 + g, -0.2, 0.2, r + g, 0, self.STEELB))
        md = o.modifiers.new('round', 'BEVEL')
        md.width = 0.014; md.segments = 5; md.limit_method = 'ANGLE'; md.angle_limit = math.radians(40); md.harden_normals = True
        return o

    def ring(self, name, x0, y0, x1, y1, r, w, z0, z1, m):
        """A thin rounded frame (outer box minus inner box), e.g. a steel trim around an opening."""
        o = self.rbox(name, x0 - w, y0 - w, x1 + w, y1 + w, z0, z1, r + w, 0.6, m)
        c = self.rbox(name + '_cut', x0, y0, x1, y1, z0 - 0.1, z1 + 0.1, r, 0, m)
        self.cut(o, c)
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

    SCALE = {'BarlowSemiCondensed-SemiBold.ttf': 1.18, 'BarlowSemiCondensed-Bold.ttf': 1.18, 'galmuri11.ttf': 1.45, 'dotgothic16.ttf': 1.1, 'pixelify_600.ttf': 1.2, 'michroma.ttf': 1.3, 'mplus_r_xb.ttf': 1.12, 'mplus_r_md.ttf': 1.05, 'mplus_r_bd.ttf': 1.05, 'dela.ttf': 1.0}

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
        k = self.rbox(name, x0, y0, x1, y1, z - 0.05, zt, r, 4.5, m or self.KEY)   # a softer roll-off edge catches the rim light
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
