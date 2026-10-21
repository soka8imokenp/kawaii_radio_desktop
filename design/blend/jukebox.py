"""
The cover window of the bay: a little jukebox stage. Sleeves stand on a black glossy floor (real reflections),
the current one faces you under a spotlight and its record slides out and spins. Changing the track: the record
slides back in, the row turns by one, the new record slides out.

  blender -b -P jukebox.py -- --out still.png [--w 1216 --h 282] [--s 3 --out-amt 1]
  blender -b -P jukebox.py -- --anim frames_dir [--w 1216 --h 282]
"""
import bpy, bmesh, math, os, sys
from mathutils import Vector, Matrix
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib
from lib import hexc

argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []


def arg(k, d=None, f=str):
    return f(argv[argv.index(k) + 1]) if k in argv else d


WW, HH = arg('--w', 1216, int), arg('--h', 282, int)
ANIM = arg('--anim')
SAMPLES = arg('--samples', 64 if ANIM else 192, int)

C = lib.Ctx('dark', WW, HH, SAMPLES, ortho=False)
sc = C.sc
sc.world.node_tree.nodes['Background'].inputs['Strength'].default_value = 0.0

COVERS = ['eve-tokyo-ghetto-mv8di1fh', 'fujii-kaze-matsuri-mv84iqkb', 'jpop-yoasobi-yoru-ni-kake-ru-3dptxh-mvdsi3h0',
          'jpop-kenshi-yonezu-kick-back-3kheep-mvdvifuv', 'kenshi-yonezu-lemon-mv84iyp8', 'creepy-nuts-bling-bang-bang-born-mv84iew9',
          'yoasobi-mv8ditp6-mv8ditp6', 'ado-kira-mv84h2ww', 'eve-underdog-mv84in6q']
GLOW = ['#9aa7c4', '#e0864a', '#ff5a7a', '#e0313f', '#3aa0b8', '#e8b931', '#c04cff', '#3b6fe0', '#c94a3a']
S = 1.3  # sleeve size

# camera
cd = bpy.data.cameras.new('cam'); cd.lens = 30; cd.sensor_width = 36
cam = bpy.data.objects.new('cam', cd); sc.collection.objects.link(cam); sc.camera = cam
cam.location = (0, -6.5, 0.78)
cam.rotation_euler = (Vector((0, 0, 0.6)) - cam.location).to_track_quat('-Z', 'Y').to_euler()
cd.dof.use_dof = True
cd.dof.focus_distance = 6.45
cd.dof.aperture_fstop = 2.2

# floor: black lacquer, mirror-like
FLOOR = C.mat('floor', hexc('#060607'), 0.12, coat=1.0, spec=0.5)
bpy.ops.mesh.primitive_plane_add(size=60, location=(0, 10, 0))
floor = bpy.context.active_object; floor.data.materials.append(FLOOR)

# back wall: a soft glow in the current cover's colour
WALL = bpy.data.materials.new('wall'); WALL.use_nodes = True
nt = WALL.node_tree
for n in list(nt.nodes):
    if n.type == 'BSDF_PRINCIPLED':
        nt.nodes.remove(n)
em = nt.nodes.new('ShaderNodeEmission')
tc = nt.nodes.new('ShaderNodeTexCoord')
gr = nt.nodes.new('ShaderNodeTexGradient'); gr.gradient_type = 'SPHERICAL'
mp = nt.nodes.new('ShaderNodeMapping'); mp.inputs['Scale'].default_value = (0.16, 0.3, 0.42); mp.inputs['Location'].default_value = (0, 0, -0.02)
nt.links.new(tc.outputs['Object'], mp.inputs['Vector']); nt.links.new(mp.outputs['Vector'], gr.inputs['Vector'])
ramp = nt.nodes.new('ShaderNodeValToRGB')
ramp.color_ramp.elements[0].color = (0.004, 0.004, 0.005, 1)
WALLCOL = ramp.color_ramp.elements[1]
nt.links.new(gr.outputs['Fac'], ramp.inputs['Fac']); nt.links.new(ramp.outputs['Color'], em.inputs['Color'])
em.inputs['Strength'].default_value = 1.0
nt.links.new(em.outputs['Emission'], nt.nodes['Material Output'].inputs['Surface'])
bpy.ops.mesh.primitive_plane_add(size=1, location=(0, 4.5, 2))
wall = bpy.context.active_object; wall.rotation_euler = (math.radians(90), 0, 0); wall.scale = (30, 8, 1)
wall.data.materials.append(WALL)

# lights
C.spot('spot', (0, -1.6, 4.2), 420, (0, 0, 0.6), 26, 0.8, (1, 0.96, 0.9), 0.4)
RIML = C.area('riml', (-4.5, 2.6, 2.2), 2.5, 120, target=(0, 0, 0.7))
RIMR = C.area('rimr', (4.5, 2.6, 2.2), 2.5, 100, (0.55, 0.7, 1.0), target=(0, 0, 0.7))
C.area('front', (0, -6, 2.4), 6, 35, target=(0, 0, 0.7))

SLEEVE = C.mat('sleeve', hexc('#efede8'), 0.6)
VINYL = bpy.data.materials.new('vinyl'); VINYL.use_nodes = True
nt = VINYL.node_tree; b = nt.nodes['Principled BSDF']
b.inputs['Base Color'].default_value = (*hexc('#0c0c0e'), 1); b.inputs['Roughness'].default_value = 0.22; b.inputs['Coat Weight'].default_value = 0.6
tc = nt.nodes.new('ShaderNodeTexCoord'); wv = nt.nodes.new('ShaderNodeTexWave'); wv.wave_type = 'RINGS'; wv.rings_direction = 'Y' if False else 'SPHERICAL'
wv.inputs['Scale'].default_value = 60; wv.inputs['Distortion'].default_value = 0.6
nt.links.new(tc.outputs['Object'], wv.inputs['Vector'])
bp = nt.nodes.new('ShaderNodeBump'); bp.inputs['Strength'].default_value = 0.25; bp.inputs['Distance'].default_value = 0.002
nt.links.new(wv.outputs['Fac'], bp.inputs['Height']); nt.links.new(bp.outputs['Normal'], b.inputs['Normal'])


def uv_plane(name, w, h, m):
    """A plane in XZ (facing -Y), centred, with 0..1 UVs."""
    bm = bmesh.new()
    vs = [bm.verts.new(v) for v in ((-w / 2, 0, -h / 2), (w / 2, 0, -h / 2), (w / 2, 0, h / 2), (-w / 2, 0, h / 2))]
    f = bm.faces.new(vs)
    uv = bm.loops.layers.uv.new()
    for l, c in zip(f.loops, ((0, 0), (1, 0), (1, 1), (0, 1))):
        l[uv].uv = c
    me = bpy.data.meshes.new(name); bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new(name, me); sc.collection.objects.link(o)
    o.data.materials.append(m)
    return o


def disc(name, r, m, segs=96):
    bm = bmesh.new()
    res = bmesh.ops.create_circle(bm, cap_ends=True, segments=segs, radius=r)
    uv = bm.loops.layers.uv.new()
    for f in bm.faces:
        for l in f.loops:
            co = l.vert.co
            l[uv].uv = (co.x / (2 * r) + 0.5, co.y / (2 * r) + 0.5)
    me = bpy.data.meshes.new(name); bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new(name, me); sc.collection.objects.link(o)
    o.data.materials.append(m)
    return o


ITEMS = []
for i, c in enumerate(COVERS):
    root = bpy.data.objects.new('item%d' % i, None); sc.collection.objects.link(root)
    cm = C.image_mat('c%d' % i, os.path.join(lib.TEX, c + '.jpg'), rough=0.45, coat=0.15)
    cm.node_tree.nodes['Principled BSDF'].inputs['Specular IOR Level'].default_value = 0.2
    body = C.rbox('sl%d' % i, 0, 0, 1, 1, 0, 1, 0, 0, SLEEVE)
    body.data.transform(Matrix.Translation((-body.data.vertices[0].co.x, 0, 0)))
    for v in body.data.vertices:
        v.co = Vector((0, 0, 0))
    bm = bmesh.new(); bmesh.ops.create_cube(bm, size=1); bm.to_mesh(body.data); bm.free()
    md = body.modifiers.new('bev', 'BEVEL'); md.width = 0.008; md.segments = 3
    body.scale = (S, 0.03, S); body.location = (0, 0, S / 2); body.parent = root
    front = uv_plane('f%d' % i, S - 0.004, S - 0.004, cm); front.location = (0, -0.0155, S / 2); front.parent = root
    # the record, inside the sleeve (slides out along +X in the sleeve's space)
    vroot = bpy.data.objects.new('v%d' % i, None); sc.collection.objects.link(vroot); vroot.parent = root
    vroot.location = (0, 0.0, S / 2)
    bm = bmesh.new(); bmesh.ops.create_cone(bm, cap_ends=True, segments=128, radius1=S * 0.47, radius2=S * 0.47, depth=0.008)
    me = bpy.data.meshes.new('vin'); bm.to_mesh(me); bm.free()
    for p in me.polygons:
        p.use_smooth = True
    vin = bpy.data.objects.new('vin%d' % i, me); sc.collection.objects.link(vin); vin.data.materials.append(VINYL)
    vin.rotation_euler = (math.radians(90), 0, 0); vin.parent = vroot
    lab = disc('lab%d' % i, S * 0.16, C.image_mat('l%d' % i, os.path.join(lib.TEX, c + '.jpg'), rough=0.5, coat=0.2))
    lab.rotation_euler = (math.radians(90), 0, 0); lab.location = (0, -0.0045, 0); lab.parent = vroot
    hole = disc('h%d' % i, S * 0.012, C.mat('hole', hexc('#d9dce1'), 0.2, metal=1)); hole.rotation_euler = (math.radians(90), 0, 0); hole.location = (0, -0.0047, 0); hole.parent = vroot
    ITEMS.append((root, vroot))


def ease(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def ease3(t):
    t = max(0.0, min(1.0, t))
    return 4 * t * t * t if t < 0.5 else 1 - (-2 * t + 2) ** 3 / 2


def pose(s, out, spin):
    """s: which cover is centred (float); out: how far the centred record is out (0..1); spin: record angle."""
    for i, (root, vroot) in enumerate(ITEMS):
        d = i - s
        ad = abs(d); sg = 1 if d >= 0 else -1
        x = sg * (ad * 1.55 if ad <= 1 else 1.55 + (ad - 1) * 0.52)
        y = min(ad, 1) * 1.0 + max(ad - 1, 0) * 0.3
        root.location = (x, y, 0)
        root.rotation_euler = (0, 0, -sg * min(ad, 1) * math.radians(70))
        near = max(0.0, 1 - ad)  # 1 when centred
        o = out * near
        vroot.location = (o * S * 0.62, 0.02, S / 2)
        vroot.rotation_euler = (0, spin, 0)
    # wall glow and rim colour follow the centred cover
    k = int(round(s)) % len(GLOW)
    col = hexc(GLOW[k])
    WALLCOL.color = (col[0] * 0.03, col[1] * 0.03, col[2] * 0.03, 1)
    RIML.data.color = col
    RIMR.data.color = (0.55, 0.7, 1.0)


C.bloom(0.3, 1.4, 0.5)

if ANIM:
    ANIM = os.path.abspath(ANIM)
    os.makedirs(ANIM, exist_ok=True)
    N = 112
    for f in range(N):
        if f < 16:
            s, out = 3, 1 - ease(f / 16)
        elif f < 58:
            s, out = 3 + ease3((f - 16) / 42), 0
        elif f < 82:
            s, out = 4, ease((f - 58) / 24)
        else:
            s, out = 4, 1
        pose(s, out, -math.radians(6.5) * f)
        sc.render.filepath = os.path.join(ANIM, '%04d.png' % f)
        bpy.ops.render.render(write_still=True)
    print('ANIM DONE')
else:
    pose(arg('--s', 3, float), arg('--out-amt', 1.0, float), math.radians(-20))
    sc.render.filepath = os.path.abspath(arg('--out', os.path.join(lib.HERE, 'jukebox.png')))
    bpy.ops.render.render(write_still=True)
    print('WROTE', sc.render.filepath)
