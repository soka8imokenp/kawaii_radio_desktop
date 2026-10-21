"""
The bay window as a bar jukebox (the Arcane kind): a glass chamber with a record changer.
Picture discs (the cover printed on the vinyl) wait in two combs; a carriage on a chrome rail lifts the playing
disc off the platter, turns it upright, files it into its slot, fetches the next one, lays it down; the tonearm
drops and the light pillars fade into the new cover's colour.

  blender -b -P jukebox2.py -- --out still.png [--w 1216 --h 286] [--frame N]
  blender -b -P jukebox2.py -- --anim dir [--w 1824 --h 430] [--from F --to F]
"""
import bpy, bmesh, math, os, sys
from mathutils import Vector, Matrix, Euler
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib
from lib import hexc

argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []


def arg(k, d=None, f=str):
    return f(argv[argv.index(k) + 1]) if k in argv else d


ANIM = arg('--anim')
WW, HH = arg('--w', 1824 if ANIM else 1208, int), arg('--h', 486 if ANIM else 316, int)
C = lib.Ctx('dark', WW, HH, arg('--samples', 56 if ANIM else 160, int), ortho=False)
sc = C.sc
sc.world.node_tree.nodes['Background'].inputs['Strength'].default_value = 0.0
sc.cycles.light_sampling_threshold = 0.02

COVERS = ['eve-tokyo-ghetto-mv8di1fh', 'fujii-kaze-matsuri-mv84iqkb', 'jpop-yoasobi-yoru-ni-kake-ru-3dptxh-mvdsi3h0',
          'jpop-kenshi-yonezu-kick-back-3kheep-mvdvifuv', 'kenshi-yonezu-lemon-mv84iyp8', 'creepy-nuts-bling-bang-bang-born-mv84iew9',
          'yoasobi-mv8ditp6-mv8ditp6', 'ado-kira-mv84h2ww', 'eve-underdog-mv84in6q', 'ado-odo-mv84hade', 'kenshi-yonezu-iris-out-mv84ixgy',
          'fujii-kaze-matsuri-mv84iqkb']
COL_OLD, COL_NEW = hexc('#ff3a4a'), hexc('#2fd6c8')
# looks: camera, light and colour. 'tubes' fixed = the pillars keep their own colour (only the halo follows the cover)
LOOKS = {
    'base': dict(cam=(0, -7.9, 2.55), at=(0, 0.25, 0.98), lens=30, f=4.0, tubes=None, steel='#2a2c31', dark='#141518', back='#141518',
                 spot=(1, 0.95, 0.88), spotE=260, pl=60, tubeS=9, ringS=4, fill=18),
    'arcane': dict(cam=(-3.3, -6.7, 2.0), at=(0.35, 0.3, 0.95), lens=34, f=2.8, tubes=('#35f0cf', '#35f0cf'), steel='#1d2a2a', dark='#0b1414', back='#0a1717',
                   spot=(1, 0.72, 0.42), spotE=340, pl=90, tubeS=7, ringS=3, fill=10),
    'wurlitzer': dict(cam=(0, -5.6, 4.3), at=(0, 0.35, 0.55), lens=32, f=4.5, tubes=('#ff9a2e', '#ff9a2e'), steel='#3a2a1e', dark='#24160d', back='#2b1a0e',
                      spot=(1, 0.86, 0.66), spotE=300, pl=70, tubeS=6, ringS=3.5, fill=22),
    'neon': dict(cam=(2.4, -5.0, 0.75), at=(-0.25, 0.3, 1.05), lens=26, f=2.4, tubes=('#ff3fa4', '#2fd6ff'), steel='#1c1a24', dark='#0e0c14', back='#100c18',
                 spot=(0.9, 0.85, 1.0), spotE=220, pl=110, tubeS=10, ringS=5, fill=8),
}
LOOK = LOOKS[arg('--look', 'base')]
CUR, NEXT = 3, 4
R = 0.5           # disc radius
PLAT = Vector((0, -0.1, 0.215))  # disc centre on the platter

# ── camera
cd = bpy.data.cameras.new('cam'); cd.lens = LOOK['lens']
cam = bpy.data.objects.new('cam', cd); sc.collection.objects.link(cam); sc.camera = cam
cam.location = LOOK['cam']
cam.rotation_euler = (Vector(LOOK['at']) - cam.location).to_track_quat('-Z', 'Y').to_euler()
cd.dof.use_dof = True; cd.dof.focus_distance = (Vector(LOOK['at']) - Vector(LOOK['cam'])).length; cd.dof.aperture_fstop = LOOK['f']

# ── materials
STEEL = C.mat('steel', hexc(LOOK['steel']), 0.32, metal=0.85, aniso=0.6, bump=(300, 0.04))
DARKM = C.mat('darkm', hexc(LOOK['dark']), 0.45, metal=0.4)
CHROME = C.mat('chromej', hexc('#e9ebee'), 0.07, metal=1.0)
BRASS = C.mat('brass', hexc('#d4a85a'), 0.22, metal=1.0, aniso=0.4)
RUBBER = C.mat('rubber', hexc('#0e0e10'), 0.7)
MATT = C.mat('platmat', hexc('#16171a'), 0.85, bump=(400, 0.05))


def glow_mat(name, col, strength):
    m = bpy.data.materials.new(name); m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        if n.type == 'BSDF_PRINCIPLED':
            nt.nodes.remove(n)
    e = nt.nodes.new('ShaderNodeEmission'); e.inputs['Color'].default_value = (*col, 1); e.inputs['Strength'].default_value = strength
    nt.links.new(e.outputs['Emission'], nt.nodes['Material Output'].inputs['Surface'])
    return m, e


TUBE, TUBE_E = glow_mat('tube', COL_OLD, 9)
TUBEL, TUBE_L = glow_mat('tubel', COL_OLD, 9)
TUBER, TUBE_R = glow_mat('tuber', COL_OLD, 9)
RING, RING_E = glow_mat('ring', COL_OLD, 4)
GLASSJ = C.mat('glassj', (1, 1, 1), 0.04, trans=1.0, ior=1.45)


def link(o):
    sc.collection.objects.link(o)
    return o


def mesh_obj(name, bm, m):
    me = bpy.data.meshes.new(name); bm.to_mesh(me); bm.free()
    for p in me.polygons:
        p.use_smooth = True
    o = link(bpy.data.objects.new(name, me)); o.data.materials.append(m)
    return o


def cylinder(name, r, h, m, loc, rot=(0, 0, 0), segs=64, bev=0.0):
    bm = bmesh.new(); bmesh.ops.create_cone(bm, cap_ends=True, segments=segs, radius1=r, radius2=r, depth=h)
    o = mesh_obj(name, bm, m); o.location = loc; o.rotation_euler = rot
    if bev:
        md = o.modifiers.new('b', 'BEVEL'); md.width = bev; md.segments = 4; md.limit_method = 'ANGLE'
    return o


def box(name, size, m, loc, rot=(0, 0, 0), bev=0.0):
    bm = bmesh.new(); bmesh.ops.create_cube(bm, size=1)
    o = mesh_obj(name, bm, m); o.scale = size; o.location = loc; o.rotation_euler = rot
    if bev:
        md = o.modifiers.new('b', 'BEVEL'); md.width = bev; md.segments = 3; md.limit_method = 'ANGLE'
    return o


# ── chamber
box('floor', (7.2, 2.6, 0.1), STEEL, (0, 0.2, -0.05))
box('back', (7.2, 0.1, 2.6), C.mat('backm', hexc(LOOK['back']), 0.6, metal=0.3), (0, 1.35, 1.25))
for i in range(-17, 18):  # art-deco fins on the back wall
    box('fin', (0.025, 0.06, 2.3), CHROME if i % 3 == 0 else STEEL, (i * 0.19, 1.29, 1.2))
# the halo ring behind the platter
bpy.ops.mesh.primitive_torus_add(major_radius=1.05, minor_radius=0.028, major_segments=128, minor_segments=16, location=(0, 1.22, 1.0), rotation=(math.radians(90), 0, 0))
ring = bpy.context.active_object; ring.data.materials.append(RING)
bpy.ops.mesh.primitive_torus_add(major_radius=1.16, minor_radius=0.03, major_segments=128, minor_segments=16, location=(0, 1.23, 1.0), rotation=(math.radians(90), 0, 0))
bpy.context.active_object.data.materials.append(CHROME)
for i in range(24):  # sunburst spokes inside the halo
    a = i / 24 * math.tau
    box('spoke', (0.012, 0.02, 0.9), STEEL, (math.cos(a) * 0.5, 1.25, 1.0 + math.sin(a) * 0.5), (0, -a + math.pi / 2, 0))
# light pillars
for sx in (-1, 1):
    x = sx * 3.05
    cylinder('tube', 0.05, 2.1, TUBEL if sx < 0 else TUBER, (x, -0.35, 1.05), segs=24)
    cylinder('sleeve', 0.11, 2.15, GLASSJ, (x, -0.35, 1.05), segs=48)
    for z in (0.02, 2.12):
        cylinder('cap', 0.14, 0.08, CHROME, (x, -0.35, z), segs=48, bev=0.01)
    box('pillar', (0.3, 2.2, 2.4), STEEL, (x + sx * 0.32, 0.25, 1.2), bev=0.02)
    L = bpy.data.lights.new('pl', 'POINT'); L.energy = LOOK['pl']; L.color = COL_OLD; L.shadow_soft_size = 0.15
    lo = link(bpy.data.objects.new('pl', L)); lo.location = (x, -0.35, 1.0); lo.visible_camera = False
    if sx < 0:
        PL_L = L
    else:
        PL_R = L
# key light on the platter and a soft top
C.spot('spot', (0, -1.4, 3.6), LOOK['spotE'], (0, -0.1, 0.2), 38, 0.9, LOOK['spot'], 0.35)
C.area('top', (0, 0.2, 3.4), 5, 90, target=(0, 0.2, 0))
C.area('frontfill', (0, -5, 1.6), 5, LOOK['fill'], target=(0, 0, 0.8))

# ── turntable
cylinder('plinth', 0.68, 0.14, DARKM, (0, -0.1, 0.07), segs=96, bev=0.02)
cylinder('platter', 0.6, 0.05, CHROME, (0, -0.1, 0.165), segs=128, bev=0.008)
platmat = cylinder('mat', 0.55, 0.012, MATT, (0, -0.1, 0.196), segs=96)
for i in range(72):  # strobe dots on the platter rim
    a = i / 72 * math.tau
    box('sd', (0.018, 0.018, 0.006), DARKM, (math.cos(a) * 0.6, -0.1 + math.sin(a) * 0.6, 0.192), (0, 0, a))
cylinder('spindle', 0.012, 0.06, CHROME, (0, -0.1, 0.24), segs=24)

# tonearm: pivot base, tube, headshell
ARM_P = Vector((0.82, 0.42, 0.2))
cylinder('armbase', 0.1, 0.12, CHROME, (ARM_P.x, ARM_P.y, 0.1), segs=48, bev=0.01)
arm = link(bpy.data.objects.new('arm', None)); arm.location = (ARM_P.x, ARM_P.y, 0.26)
t = cylinder('armtube', 0.012, 0.78, CHROME, (0.39, 0, 0), (0, math.radians(90), 0), 24); t.parent = arm
h = box('headshell', (0.09, 0.05, 0.02), DARKM, (0.8, 0, -0.025), bev=0.005); h.parent = arm
cw = cylinder('cw', 0.04, 0.08, CHROME, (-0.1, 0, 0), (0, math.radians(90), 0), 32, 0.006); cw.parent = arm

# ── picture discs
def picture_disc(i):
    m = bpy.data.materials.new('pd%d' % i); m.use_nodes = True
    nt = m.node_tree; b = nt.nodes['Principled BSDF']
    tx = nt.nodes.new('ShaderNodeTexImage'); tx.image = bpy.data.images.load(os.path.join(lib.TEX, COVERS[i] + '.jpg'))
    nt.links.new(tx.outputs['Color'], b.inputs['Base Color'])
    b.inputs['Roughness'].default_value = 0.18; b.inputs['Coat Weight'].default_value = 1.0; b.inputs['Coat Roughness'].default_value = 0.05
    # grooves catch the light
    tc = nt.nodes.new('ShaderNodeTexCoord'); wv = nt.nodes.new('ShaderNodeTexWave'); wv.wave_type = 'RINGS'; wv.rings_direction = 'SPHERICAL'
    wv.inputs['Scale'].default_value = 90; nt.links.new(tc.outputs['Object'], wv.inputs['Vector'])
    bp = nt.nodes.new('ShaderNodeBump'); bp.inputs['Strength'].default_value = 0.12; bp.inputs['Distance'].default_value = 0.001
    nt.links.new(wv.outputs['Fac'], bp.inputs['Height']); nt.links.new(bp.outputs['Normal'], b.inputs['Coat Normal'])
    bm = bmesh.new()
    bmesh.ops.create_circle(bm, cap_ends=True, segments=96, radius=R)
    uv = bm.loops.layers.uv.new()
    for f in bm.faces:
        for l in f.loops:
            l[uv].uv = (l.vert.co.x / (2 * R) + 0.5, l.vert.co.y / (2 * R) + 0.5)
    o = mesh_obj('disc%d' % i, bm, m)
    s = o.modifiers.new('s', 'SOLIDIFY'); s.thickness = 0.012; s.offset = 0
    o.data.materials.append(C.mat('edge%d' % i, hexc('#101012'), 0.3))
    s.material_offset_rim = 1
    hole = cylinder('hole', 0.012, 0.016, CHROME, (0, 0, 0), segs=16); hole.parent = o
    return o


DISCS = [picture_disc(i) for i in range(len(COVERS))]

# combs: six slots each side; the slot order skips the disc on the platter
SLOTS = {}
left = [i for i in range(0, 6)]
right = [i for i in range(6, 12)]
for k, i in enumerate(left):
    SLOTS[i] = (Vector((-1.18 - 0.2 * k, 0.22, R + 0.03)), math.radians(52))
for k, i in enumerate(right):
    SLOTS[i] = (Vector((1.18 + 0.2 * k, 0.22, R + 0.03)), math.radians(-52))
# NEXT sits first on the right
SLOTS[NEXT], SLOTS[6] = SLOTS[6], SLOTS[NEXT]
for side, xs in ((-1, [-1.18 - 0.2 * k for k in range(6)]), (1, [1.18 + 0.2 * k for k in range(6)])):
    box('comb', (1.35, 0.42, 0.05), STEEL, (side * 1.68, 0.22, 0.025), bev=0.01)
    for x in xs:
        for dy in (-0.16, 0.16):
            box('tooth', (0.05, 0.05, 0.1), CHROME, (x + side * 0.1, 0.22 + dy, 0.08), bev=0.008)

# ── the changer: rail, carriage, rod, gripper
for y in (-0.02, 0.18):
    cylinder('rail', 0.035, 6.2, CHROME, (0, y, 2.02), (0, math.radians(90), 0), 32)
carriage = link(bpy.data.objects.new('carriage', None))
box('cblock', (0.34, 0.36, 0.16), BRASS, (0, 0.08, 0), bev=0.02).parent = carriage
for dx in (-0.12, 0.12):
    cylinder('wheel', 0.045, 0.04, DARKM, (dx, -0.02, 0.06), (math.radians(90), 0, 0), 24).parent = carriage
rod = cylinder('rod', 0.022, 1.0, CHROME, (0, 0, 0), segs=24)
rod2 = cylinder('rodsleeve', 0.04, 0.3, BRASS, (0, 0, 0), segs=24, bev=0.006)
head = link(bpy.data.objects.new('head', None))  # sits at the disc centre, turns only about Z
BARZ = R + 0.13
hb = box('hbar', (2 * R + 0.34, 0.08, 0.07), BRASS, (0, 0, BARZ), bev=0.014); hb.parent = head
FING = []
for sx in (-1, 1):
    f = link(bpy.data.objects.new('finger', None)); f.parent = head
    box('fbody', (0.07, 0.09, BARZ + 0.03), BRASS, (0, 0, BARZ / 2 - 0.0), bev=0.01).parent = f
    box('fpad', (0.035, 0.09, 0.06), RUBBER, (-sx * 0.012, 0, 0)).parent = f
    FING.append((f, sx))
lamp = cylinder('hlamp', 0.025, 0.02, TUBE, (0, -0.05, BARZ), (math.radians(90), 0, 0), 16); lamp.parent = head


# ── animation
def ease(t):
    t = max(0.0, min(1.0, t)); return t * t * (3 - 2 * t)


def seg(f, a, b):
    return ease((f - a) / (b - a))


def lerp(a, b, t):
    return a + (b - a) * t


PARK = 1.12
SLOT_Z = R + 0.03
UP = math.radians(90)


def disc_m(pos, rx, rz):
    return Matrix.Translation(pos) @ Euler((rx, 0, rz), 'XYZ').to_matrix().to_4x4()


def pose(f):
    oldp, oldphi = SLOTS[CUR]
    newp, newphi = SLOTS[NEXT]
    PZ = PLAT.z
    x, y, z, rx, rz, grip, carry = 0.0, PLAT.y, PARK, 0.0, 0.0, 0.0, None
    if f < 38:
        pass
    elif f < 58:
        z = lerp(PARK, PZ, seg(f, 38, 54)); grip = seg(f, 52, 58)
    elif f < 74:
        z = lerp(PZ, PARK, seg(f, 58, 74)); grip = 1; carry = CUR
    elif f < 98:
        t = seg(f, 74, 98)
        x, y = lerp(0, oldp.x, t), lerp(PLAT.y, oldp.y, t); z = PARK + 0.08 * math.sin(math.pi * t)
        rx, rz, grip, carry = lerp(0, UP, t), lerp(0, oldphi, t), 1, CUR
    elif f < 112:
        x, y, rx, rz = oldp.x, oldp.y, UP, oldphi
        z = lerp(PARK, SLOT_Z, seg(f, 98, 108)); grip = 1 - seg(f, 106, 112); carry = CUR if f < 108 else None
    elif f < 122:
        x, y, rx, rz = oldp.x, oldp.y, UP, oldphi
        z = lerp(SLOT_Z, PARK, seg(f, 112, 122))
    elif f < 150:
        t = seg(f, 122, 140)
        x, y, rx, rz = lerp(oldp.x, newp.x, t), lerp(oldp.y, newp.y, t), UP, lerp(oldphi, newphi, t)
        z = lerp(PARK, SLOT_Z, seg(f, 138, 146)); grip = seg(f, 144, 150)
    elif f < 166:
        x, y, rx, rz, grip, carry = newp.x, newp.y, UP, newphi, 1, NEXT
        z = lerp(SLOT_Z, PARK, seg(f, 152, 166))
    elif f < 190:
        t = seg(f, 166, 190)
        x, y = lerp(newp.x, 0, t), lerp(newp.y, PLAT.y, t); z = PARK + 0.08 * math.sin(math.pi * t)
        rx, rz, grip, carry = lerp(UP, 0, t), lerp(newphi, 0, t), 1, NEXT
    elif f < 206:
        z = lerp(PARK, PZ, seg(f, 190, 202)); grip = 1 - seg(f, 200, 206); carry = NEXT if f < 202 else None
    else:
        z = lerp(PZ, PARK, seg(f, 206, 222))
    pos = Vector((x, y, z))
    head.matrix_world = Matrix.Translation(pos) @ Euler((0, 0, rz)).to_matrix().to_4x4()
    for fo, sx in FING:
        fo.location = (sx * lerp(R + 0.1, R + 0.03, grip), 0, 0)
    carriage.location = (x, 0.08, 1.9)
    top = Vector((x, y, 1.92)); bot = pos + Vector((0, 0, BARZ + 0.03))
    rod.location = (top + bot) / 2; rod.scale = (1, 1, max((top - bot).length, 0.05))
    rod2.location = (x, y, 1.78)
    spin_old = -math.radians(6.5) * f * max(0.0, 1 - seg(f, 20, 40))
    spin_new = -math.radians(6.5) * max(0, f - 214) * seg(f, 214, 240)
    for i, d in enumerate(DISCS):
        if i == carry:
            d.matrix_world = disc_m(pos, rx, rz)
        elif (i == CUR and f < 58) or (i == NEXT and f >= 202):
            d.matrix_world = disc_m(PLAT, 0, spin_old if i == CUR else spin_new)
        else:
            p_, phi = SLOTS[i]
            d.matrix_world = disc_m(p_, UP, phi)
    a_rest, a_play = math.radians(-86), math.radians(-126)
    if f < 40:
        a = lerp(a_play, a_rest, seg(f, 24, 40)); lift = seg(f, 20, 26)
    elif f < 208:
        a, lift = a_rest, 1
    else:
        a = lerp(a_rest, a_play, seg(f, 208, 226)); lift = 1 - seg(f, 224, 232)
    arm.rotation_euler = (0, -0.06 * lift - 0.02, a)
    k = seg(f, 150, 214)
    col = [lerp(COL_OLD[j], COL_NEW[j], k) for j in range(3)]
    RING_E.inputs['Color'].default_value = (*col, 1)
    if LOOK['tubes']:
        cl, cr = hexc(LOOK['tubes'][0]), hexc(LOOK['tubes'][1])
        TUBE_L.inputs['Color'].default_value = (*cl, 1); TUBE_R.inputs['Color'].default_value = (*cr, 1)
        PL_L.color = cl; PL_R.color = cr
    else:
        TUBE_L.inputs['Color'].default_value = (*col, 1); TUBE_R.inputs['Color'].default_value = (*col, 1)
        PL_L.color = col; PL_R.color = col
    flick = 0.45 if (214 <= f < 224 and f % 3 == 0) else 1.0
    TUBE_L.inputs['Strength'].default_value = LOOK['tubeS'] * flick; TUBE_R.inputs['Strength'].default_value = LOOK['tubeS'] * flick; RING_E.inputs['Strength'].default_value = LOOK['ringS'] * flick


C.bloom(0.45, 1.0, 0.55)

if ANIM:
    ANIM = os.path.abspath(ANIM); os.makedirs(ANIM, exist_ok=True)
    f0, f1 = arg('--from', 0, int), arg('--to', 260, int)
    for f in range(f0, f1):
        pose(f)
        sc.render.filepath = os.path.join(ANIM, '%04d.png' % f)
        bpy.ops.render.render(write_still=True)
    print('ANIM DONE')
else:
    pose(arg('--frame', 0, int))
    sc.render.filepath = os.path.abspath(arg('--out', os.path.join(lib.HERE, 'jukebox2.png')))
    bpy.ops.render.render(write_still=True)
    print('WROTE', sc.render.filepath)
