"""
Three modern ideas for the cover window, each caught mid-change.
  blender -b -P concepts.py -- --c trivision|acrylic|hologram --t 0.45 --out file.png [--w --h --samples]
"""
import bpy, bmesh, math, os, sys, random
from mathutils import Vector, Matrix, Euler
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib
from lib import hexc

argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []


def arg(k, d=None, f=str):
    return f(argv[argv.index(k) + 1]) if k in argv else d


CON = arg('--concept', 'trivision')
TT = arg('--t', 0.45, float)
C = lib.Ctx('dark', arg('--w', 1456, int), arg('--h', 388, int), arg('--samples', 64, int), ortho=False)
sc = C.sc
sc.world.node_tree.nodes['Background'].inputs['Strength'].default_value = 0.0
COV = ['jpop-kenshi-yonezu-kick-back-3kheep-mvdvifuv', 'kenshi-yonezu-lemon-mv84iyp8', 'jpop-yoasobi-yoru-ni-kake-ru-3dptxh-mvdsi3h0']
GLOW_A, GLOW_B = hexc('#ff4a4a'), hexc('#41c7d9')


def link(o):
    sc.collection.objects.link(o); return o


def camera(loc, at, lens, fstop=3.2):
    cd = bpy.data.cameras.new('cam'); cd.lens = lens
    cam = link(bpy.data.objects.new('cam', cd)); sc.camera = cam
    cam.location = loc; cam.rotation_euler = (Vector(at) - Vector(loc)).to_track_quat('-Z', 'Y').to_euler()
    cd.dof.use_dof = True; cd.dof.focus_distance = (Vector(at) - Vector(loc)).length; cd.dof.aperture_fstop = fstop


def haze(density=0.035, color=(1, 1, 1)):
    """A little air in the box, so light beams show (the Arcane feel)."""
    w = sc.world.node_tree
    vs = w.nodes.new('ShaderNodeVolumeScatter'); vs.inputs['Density'].default_value = density; vs.inputs['Color'].default_value = (*color, 1)
    w.links.new(vs.outputs['Volume'], w.nodes['World Output'].inputs['Volume'])
    sc.cycles.volume_bounces = 1
    sc.cycles.volume_step_rate = 4


def emis(name, col, strength):
    m = bpy.data.materials.new(name); m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        if n.type == 'BSDF_PRINCIPLED':
            nt.nodes.remove(n)
    e = nt.nodes.new('ShaderNodeEmission'); e.inputs['Color'].default_value = (*col, 1); e.inputs['Strength'].default_value = strength
    nt.links.new(e.outputs['Emission'], nt.nodes['Material Output'].inputs['Surface'])
    return m


def tex_mat(name, path, rough=0.35, coat=0.3, emit=0.0):
    m = bpy.data.materials.new(name); m.use_nodes = True
    nt = m.node_tree; b = nt.nodes['Principled BSDF']
    t = nt.nodes.new('ShaderNodeTexImage'); t.image = bpy.data.images.load(path); t.extension = 'EXTEND'
    nt.links.new(t.outputs['Color'], b.inputs['Base Color'])
    b.inputs['Roughness'].default_value = rough; b.inputs['Coat Weight'].default_value = coat
    if emit:
        nt.links.new(t.outputs['Color'], b.inputs['Emission Color']); b.inputs['Emission Strength'].default_value = emit
    return m


def box(name, size, m, loc, rot=(0, 0, 0), bev=0.0):
    bm = bmesh.new(); bmesh.ops.create_cube(bm, size=1)
    me = bpy.data.meshes.new(name); bm.to_mesh(me); bm.free()
    o = link(bpy.data.objects.new(name, me)); o.data.materials.append(m)
    o.scale = size; o.location = loc; o.rotation_euler = rot
    if bev:
        md = o.modifiers.new('b', 'BEVEL'); md.width = bev; md.segments = 4; md.limit_method = 'ANGLE'
        for p in me.polygons:
            p.use_smooth = True
        md.harden_normals = True
    return o


def ease_back(t):
    t = max(0.0, min(1.0, t)); c1 = 1.70158; c3 = c1 + 1
    return 1 + c3 * (t - 1) ** 3 + c1 * (t - 1) ** 2


GRAPH = C.mat('g', hexc('#17181c'), 0.42, metal=0.3, bump=(600, 0.05))
ALU = C.mat('alu', hexc('#b9bec6'), 0.28, metal=1.0, aniso=0.7)
FLOOR = C.mat('floor', hexc('#0c0c0e'), 0.16, coat=1.0)

if CON == 'trivision':
    # 28 triangular slats; each face carries a vertical slice of one cover. A wave turns them 120° with a bounce.
    camera((-2.4, -8.6, 1.5), (0.15, 0, 1.2), 30, 5.0)
    N, Wd, Hh = 28, 4.2, 2.2
    w = Wd / N
    r = w / math.sqrt(3) * 1.02
    mats = [tex_mat('cv%d' % k, os.path.join(lib.TEX, COV[k] + '.jpg'), 0.32, 0.5) for k in range(3)]
    for i in range(N):
        bm = bmesh.new()
        uv = bm.loops.layers.uv.new()
        ang = [math.radians(-90 + 120 * k) for k in range(3)]  # face normals; face 0 points to the camera (-Y)
        pts = [Vector((math.cos(a + math.radians(60)) * r, math.sin(a + math.radians(60)) * r)) for a in ang]
        pts = [Vector((math.cos(math.radians(-30 + 120 * k)) * r, math.sin(math.radians(-30 + 120 * k)) * r)) for k in range(3)]
        top = [bm.verts.new((p.x, p.y, Hh / 2)) for p in pts]
        bot = [bm.verts.new((p.x, p.y, -Hh / 2)) for p in pts]
        bm.faces.new(top); bm.faces.new(list(reversed(bot)))
        for k in range(3):
            a, b2 = k, (k + 1) % 3
            f = bm.faces.new((bot[a], bot[b2], top[b2], top[a]))
            # which cover: the face whose outward normal points -Y at rotation 0 shows cover 0, the next 1, ...
            n = ((pts[a] + pts[b2]) / 2).normalized()
            angle = math.degrees(math.atan2(n.y, n.x)) % 360
            k_cov = {270: 0, 30: 1, 150: 2}[min((270, 30, 150), key=lambda v: abs(((angle - v + 180) % 360) - 180))]
            f.material_index = k_cov
            u0, u1 = i / N, (i + 1) / N
            # left→right as seen when this face faces the camera
            for l, (uu, vv) in zip(f.loops, ((u0, 0), (u1, 0), (u1, 1), (u0, 1))):
                l[uv].uv = (uu, vv)
        me = bpy.data.meshes.new('slat'); bm.to_mesh(me); bm.free()
        o = link(bpy.data.objects.new('slat', me))
        for m in mats:
            o.data.materials.append(m)
        md = o.modifiers.new('b', 'BEVEL'); md.width = 0.004; md.segments = 2
        o.location = (-Wd / 2 + w * (i + 0.5), 0, 1.2)
        # the wave: cover 0 → cover 1, starting left
        t = (TT * 1.6 - i / N * 0.6)
        o.rotation_euler = (0, 0, -math.radians(120) * ease_back(t / 0.45))
    # frame: aluminium bezel with LED strips top and bottom
    for z, nm in ((2.38, 't'), (0.02, 'b')):
        box('rail' + nm, (Wd + 0.3, 0.5, 0.16), ALU, (0, 0.05, z), bev=0.02)
        box('led' + nm, (Wd, 0.02, 0.02), emis('le' + nm, (1, 0.85, 0.7), 18), (0, -0.21, z + (-0.09 if nm == 't' else 0.09)))
    box('back', (Wd + 0.4, 0.1, 2.6), GRAPH, (0, 0.45, 1.2))
    box('floor', (14, 8, 0.1), FLOOR, (0, 2, -0.08))
    C.area('key', (-2.5, -3, 3.8), 3, 260, (1, 0.93, 0.85), target=(0, 0, 1.2))
    C.area('rim', (3.5, 1.2, 2.2), 2, 140, GLOW_B, target=(0, 0, 1.2))
    C.spot('beam', (0, -0.6, 3.4), 260, (0, 0, 1.0), 30, 0.6, (1, 0.9, 0.8), 0.05)
    haze(0.025)

elif CON == 'acrylic':
    # thick acrylic blocks with the cover printed on the back, edge-lit from a slot; the front one lifts away
    camera((-3.0, -9.0, 2.3), (0.25, 0.6, 1.25), 30, 4.0)
    ACR = C.mat('acr', (1, 1, 1), 0.02, trans=1.0, ior=1.49, spec=0.6)
    box('base', (5.2, 2.6, 0.24), ALU, (0, 0.6, 0.0), bev=0.04)
    slot = box('slot', (4.6, 2.2, 0.02), emis('slotglow', (1, 0.88, 0.75), 2.2), (0, 0.6, 0.125))
    for k, c in enumerate([COV[0], COV[1], COV[2], 'fujii-kaze-matsuri-mv84iqkb', 'eve-tokyo-ghetto-mv8di1fh']):
        y = 0.1 + k * 0.42
        lift = 0
        if k == 0:
            lift = 0.75 * min(1, TT * 1.6)
        blk = box('blk%d' % k, (1.9, 0.2, 1.9), ACR, (0.0 + k * 0.18, y, 1.08 + lift), (math.radians(-6 if k == 0 else 0), 0, 0), bev=0.03)
        img = tex_mat('a%d' % k, os.path.join(lib.TEX, c + '.jpg'), 0.4, 0.0, emit=0.25)
        bm = bmesh.new()
        vs = [bm.verts.new(v) for v in ((-0.88, 0, -0.88), (0.88, 0, -0.88), (0.88, 0, 0.88), (-0.88, 0, 0.88))]
        f = bm.faces.new(vs); uvl = bm.loops.layers.uv.new()
        for l, cc in zip(f.loops, ((0, 0), (1, 0), (1, 1), (0, 1))):
            l[uvl].uv = cc
        me = bpy.data.meshes.new('pr'); bm.to_mesh(me); bm.free()
        pr = link(bpy.data.objects.new('print', me)); pr.data.materials.append(img)
        pr.parent = blk; pr.location = (0, 0.42, 0); pr.scale = (1 / 1.9 * 1.9, 1, 1)
        pr.matrix_parent_inverse = blk.matrix_world.inverted() if False else Matrix()
        pr.location = (0, 0.0, 0); pr.scale = (1 / 1.9, 1 / 0.2, 1 / 1.9)
        pr.location = (0, 0.45, 0)
    box('back', (8, 0.1, 4), GRAPH, (0, 3.0, 1.5))
    box('floor', (14, 8, 0.1), FLOOR, (0, 2, -0.17))
    C.area('key', (-3, -2.5, 4), 3, 220, (1, 0.93, 0.85), target=(0, 0.5, 1))
    C.area('rim', (3.6, 2.5, 2.5), 2, 160, GLOW_A, target=(0, 0.5, 1.2))
    C.area('rim2', (-3.6, 3, 2), 2, 120, GLOW_B, target=(0, 0.5, 1.2))
    haze(0.02)

else:  # hologram
    camera((-2.0, -8.6, 1.8), (0.1, 0.3, 1.3), 30, 4.0)
    img_a = bpy.data.images.load(os.path.join(lib.TEX, COV[0] + '.jpg'))
    img_b = bpy.data.images.load(os.path.join(lib.TEX, COV[1] + '.jpg'))
    G = 72
    S = 2.2 / G

    def sample(img):
        w, h = img.size
        px = list(img.pixels)
        out = []
        for j in range(G):
            row = []
            for i in range(G):
                x = int((i + 0.5) / G * w); y = int((j + 0.5) / G * h)
                k = (y * w + x) * 4
                row.append((px[k], px[k + 1], px[k + 2]))
            out.append(row)
        return out

    A, B = sample(img_a), sample(img_b)
    random.seed(4)
    bm = bmesh.new()
    col = bm.loops.layers.color.new('col')
    t = TT
    for j in range(G):
        for i in range(G):
            # each pixel flips from A to B at its own moment, flying back in depth on the way
            d = (i / G * 0.6 + random.random() * 0.4)
            k = max(0.0, min(1.0, (t * 1.6 - d * 0.6) / 0.4))
            ca, cb = A[j][i], B[j][i]
            c = [ca[n] + (cb[n] - ca[n]) * k for n in range(3)]
            fly = math.sin(k * math.pi) * (0.6 + random.random() * 1.4)
            x = -1.1 + (i + 0.5) * S
            z = 0.15 + (j + 0.5) * S
            y = fly
            z += math.sin(k * math.pi) * (random.random() - 0.5) * 0.4
            s = S * 0.42
            vs = [bm.verts.new((x + dx * s, y, z + dz * s)) for dx, dz in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
            f = bm.faces.new(vs)
            for l in f.loops:
                l[col] = (*c, 1)
    me = bpy.data.meshes.new('holo'); bm.to_mesh(me); bm.free()
    ho = link(bpy.data.objects.new('holo', me))
    m = bpy.data.materials.new('holom'); m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        if n.type == 'BSDF_PRINCIPLED':
            nt.nodes.remove(n)
    at = nt.nodes.new('ShaderNodeVertexColor'); at.layer_name = 'col'
    e = nt.nodes.new('ShaderNodeEmission'); e.inputs['Strength'].default_value = 2.2
    nt.links.new(at.outputs['Color'], e.inputs['Color'])
    nt.links.new(e.outputs['Emission'], nt.nodes['Material Output'].inputs['Surface'])
    ho.data.materials.append(m)
    # projector puck below, its beam visible in the haze
    box('puck', (2.6, 1.2, 0.12), ALU, (0, 0, 0.0), bev=0.04)
    box('lens', (2.3, 0.9, 0.01), emis('lensg', (0.6, 0.85, 1.0), 6), (0, 0, 0.065))
    C.spot('beam', (0, 0, 0.05), 900, (0, 0, 3), 70, 0.4, (0.55, 0.8, 1.0), 0.6)
    box('back', (10, 0.1, 5), GRAPH, (0, 2.4, 2))
    box('floor', (14, 8, 0.1), FLOOR, (0, 2, -0.1))
    C.area('rim', (3.4, 1.0, 2.6), 2, 90, GLOW_B, target=(0, 0, 1.2))
    haze(0.05, (0.85, 0.92, 1.0))

C.bloom(0.5, 0.9, 0.6)
sc.render.filepath = os.path.abspath(arg('--out', os.path.join(lib.HERE, '..', 'm-%s.png' % CON)))
bpy.ops.render.render(write_still=True)
print('WROTE', sc.render.filepath)
