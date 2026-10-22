"""Thermal paper texture for the TAVSIF receipt (the app lays it over the HTML receipt with multiply).

  blender -b -P paper.py -- --out paper.png [--w 1024 --h 2048]

Procedural crumpled paper, the classic recipe: two 4D Voronoi (F2) distorted by noise through Linear Light,
multiplied together and softened with RGB curves -> large folds; a very fine noise (scale 500+) with a small
distance -> the cellulose grain. Roughness 1.0. Lit by a grazing light so the relief reads in a flat picture.
A receipt is only lightly handled, so the folds are kept shallow.
"""
import bpy, sys, os, math

argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []


def arg(k, d, f=str):
    return f(argv[argv.index(k) + 1]) if k in argv else d


OUT = os.path.abspath(arg('--out', 'paper.png'))
W, H = arg('--w', 1024, int), arg('--h', 2048, int)

bpy.ops.wm.read_factory_settings(use_empty=True)
sc = bpy.context.scene
sc.render.engine = 'CYCLES'
sc.cycles.samples = 64
sc.cycles.use_denoising = True
sc.render.resolution_x, sc.render.resolution_y = W, H
sc.view_settings.view_transform = 'Standard'
try:
    prefs = bpy.context.preferences.addons['cycles'].preferences
    prefs.compute_device_type = 'OPTIX'; prefs.get_devices()
    for d in prefs.devices:
        d.use = True
    sc.cycles.device = 'GPU'
except Exception:
    pass
world = bpy.data.worlds.new('w'); sc.world = world; world.use_nodes = True
world.node_tree.nodes['Background'].inputs['Strength'].default_value = 0.0

# the sheet
aspect = H / W
bpy.ops.mesh.primitive_plane_add(size=2)
pl = bpy.context.object
pl.scale = (1, aspect, 1)
bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)   # round fold cells, not stretched ones
m = bpy.data.materials.new('paper'); m.use_nodes = True
nt = m.node_tree; N = nt.nodes; L = nt.links
b = N['Principled BSDF']
b.inputs['Base Color'].default_value = (1, 1, 1, 1)
b.inputs['Roughness'].default_value = 1.0
b.inputs['Specular IOR Level'].default_value = 0.2
tc = N.new('ShaderNodeTexCoord')


def noise(scale, detail, rough, dist=0.0):
    n = N.new('ShaderNodeTexNoise'); n.noise_dimensions = '4D'
    n.inputs['Scale'].default_value = scale; n.inputs['Detail'].default_value = detail
    n.inputs['Roughness'].default_value = rough; n.inputs['Distortion'].default_value = dist
    n.inputs['W'].default_value = 3.7
    L.new(tc.outputs['Object'], n.inputs['Vector'])
    return n


def voronoi(scale, w, src):
    v = N.new('ShaderNodeTexVoronoi'); v.voronoi_dimensions = '4D'; v.feature = 'F2'
    v.inputs['Scale'].default_value = scale; v.inputs['W'].default_value = w
    L.new(src, v.inputs['Vector'])
    return v


def distorted(scale_noise, amount):
    # Linear Light the coordinates with a noise: folds wander instead of being straight cells
    n = noise(scale_noise, 4, 0.55)
    mix = N.new('ShaderNodeMix'); mix.data_type = 'RGBA'; mix.blend_type = 'LINEAR_LIGHT'
    mix.inputs['Factor'].default_value = amount
    L.new(tc.outputs['Object'], mix.inputs[6]); L.new(n.outputs['Color'], mix.inputs[7])
    return mix.outputs[2]


v1 = voronoi(2.2, 1.3, distorted(1.6, 0.12))
v2 = voronoi(3.4, 7.9, distorted(2.4, 0.10))
mul = N.new('ShaderNodeMix'); mul.data_type = 'RGBA'; mul.blend_type = 'MULTIPLY'; mul.inputs['Factor'].default_value = 1.0
L.new(v1.outputs['Distance'], mul.inputs[6]); L.new(v2.outputs['Distance'], mul.inputs[7])
curve = N.new('ShaderNodeRGBCurve')   # soften the ridges so the folds look organic, not faceted
cm = curve.mapping.curves[3]
cm.points[0].location = (0, 0.08); cm.points[1].location = (1, 0.8)
cm.points.new(0.35, 0.42)
curve.mapping.update()
L.new(mul.outputs[2], curve.inputs['Color'])
bfold = N.new('ShaderNodeBump'); bfold.inputs['Strength'].default_value = 0.22; bfold.inputs['Distance'].default_value = 0.06
L.new(curve.outputs['Color'], bfold.inputs['Height'])
fine = noise(520, 6, 0.7)
bgrain = N.new('ShaderNodeBump'); bgrain.inputs['Strength'].default_value = 0.5; bgrain.inputs['Distance'].default_value = 0.002
L.new(fine.outputs['Fac'], bgrain.inputs['Height'])
L.new(bfold.outputs['Normal'], bgrain.inputs['Normal'])
L.new(bgrain.outputs['Normal'], b.inputs['Normal'])
pl.data.materials.append(m)

# camera straight down, grazing light from the upper left + soft fill
cd = bpy.data.cameras.new('c'); cd.type = 'ORTHO'; cd.ortho_scale = 2 * aspect
cam = bpy.data.objects.new('c', cd); cam.location = (0, 0, 5); sc.collection.objects.link(cam); sc.camera = cam
for nm, loc, e, sz in (('key', (-5, 4, 1.6), 900, 3), ('fill', (4, -3, 6), 160, 8)):
    ld = bpy.data.lights.new(nm, 'AREA'); ld.energy = e; ld.size = sz
    lo = bpy.data.objects.new(nm, ld); lo.location = loc; sc.collection.objects.link(lo)
    from mathutils import Vector
    lo.rotation_euler = (Vector((0, 0, 0)) - Vector(loc)).to_track_quat('-Z', 'Y').to_euler()

sc.render.filepath = OUT
bpy.ops.render.render(write_still=True)
print('WROTE', OUT)
