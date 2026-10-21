"""
KR-01 DESK, main screen, as a real object.

Run:  blender -b -P scene.py -- [--preview] [--out file.png]
Orthographic top-down camera: 1 Blender unit = 100 px, the frame is exactly 1600 x 1000, so every control lands
on its UI coordinate and can later be cut into sprites (rest / pressed) for the app.
"""
import bpy, bmesh, math, os, sys
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
FONTS = os.path.join(HERE, '..', 'fonts')
TEX = os.path.join(HERE, 'tex')
argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
PREVIEW = '--preview' in argv
OUT = argv[argv.index('--out') + 1] if '--out' in argv else os.path.join(HERE, '..', 'f-render.png')

W, H = 1600, 1000


def P(x, y):
    """UI pixels → world XY."""
    return x / 100 - W / 200, H / 200 - y / 100


# ───────────────────────────── scene
bpy.ops.wm.read_factory_settings(use_empty=True)
sc = bpy.context.scene
sc.render.engine = 'CYCLES'
sc.render.resolution_x, sc.render.resolution_y = (W // 2, H // 2) if PREVIEW else (W, H)
sc.render.resolution_percentage = 100
sc.cycles.samples = 48 if PREVIEW else 256
sc.cycles.use_denoising = True
sc.cycles.max_bounces = 8
sc.cycles.transmission_bounces = 8
sc.view_settings.view_transform = 'Standard'
sc.view_settings.look = 'None'
sc.view_settings.exposure = 0.0
try:
    prefs = bpy.context.preferences.addons['cycles'].preferences
    for kind in ('OPTIX', 'CUDA'):
        try:
            prefs.compute_device_type = kind
            prefs.get_devices()
            devs = [d for d in prefs.devices if d.type == kind]
            if devs:
                for d in prefs.devices:
                    d.use = d.type == kind
                sc.cycles.device = 'GPU'
                print('GPU:', kind, [d.name for d in devs])
                break
        except Exception as e:
            print('no', kind, e)
except Exception as e:
    print('GPU setup failed', e)

cam_data = bpy.data.cameras.new('cam')
cam_data.type = 'ORTHO'
cam_data.ortho_scale = W / 100
cam = bpy.data.objects.new('cam', cam_data)
cam.location = (0, 0, 20)
sc.collection.objects.link(cam)
sc.camera = cam
cam_data.clip_end = 100

# world: Blender's own studio HDRI for reflections, low
world = bpy.data.worlds.new('w')
sc.world = world
world.use_nodes = True
wn = world.node_tree.nodes
bg = wn['Background']
bg.inputs['Strength'].default_value = 0.22
hdr = None
for root in [os.path.dirname(bpy.app.binary_path)]:
    for dp, dn, fn in os.walk(root):
        if 'studio.exr' in fn and 'world' in dp:
            hdr = os.path.join(dp, 'studio.exr')
            break
if False:
    env = wn.new('ShaderNodeTexEnvironment')
    env.image = bpy.data.images.load(hdr)
    world.node_tree.links.new(env.outputs['Color'], bg.inputs['Color'])
else:
    bg.inputs['Color'].default_value = (0.78, 0.8, 0.85, 1)


def area(name, loc, size, energy, color=(1, 1, 1), target=(0, 0, 0)):
    d = bpy.data.lights.new(name, 'AREA')
    d.size = size
    d.energy = energy
    d.color = color
    o = bpy.data.objects.new(name, d)
    o.visible_camera = False
    o.visible_glossy = name != 'drumlight'
    o.location = loc
    sc.collection.objects.link(o)
    dirv = Vector(target) - Vector(loc)
    o.rotation_euler = dirv.to_track_quat('-Z', 'Y').to_euler()
    return o


# key light from the top-left (the phone's shadows fall down-right), soft fill, cool rim
area('key', (-9, 9, 11), 9, 1700, (1.0, 0.98, 0.95))
area('fill', (10, -6, 9), 12, 450, (0.92, 0.95, 1.0))
area('top', (0, 0, 14), 18, 250)

# ───────────────────────────── materials
def mat(name, color, rough=0.5, metal=0.0, coat=0.0, emit=None, estr=0.0, trans=0.0, ior=1.45, aniso=0.0, bump=None, spec=0.5):
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


def hexc(h):
    h = h.lstrip('#')
    c = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    return tuple(x ** 2.2 for x in c)  # sRGB → linear


def dotted(m, pitch, dot, ink, mix=0.9):
    """Print the phone's dot grid onto a material (object coordinates, pitch in units)."""
    nt = m.node_tree
    b = nt.nodes['Principled BSDF']
    tc = nt.nodes.new('ShaderNodeTexCoord')
    mp = nt.nodes.new('ShaderNodeMapping')
    mp.inputs['Scale'].default_value = (1 / pitch, 1 / pitch, 1)
    nt.links.new(tc.outputs['Object'], mp.inputs['Vector'])
    fr = nt.nodes.new('ShaderNodeVectorMath'); fr.operation = 'FRACTION'
    nt.links.new(mp.outputs['Vector'], fr.inputs[0])
    sub = nt.nodes.new('ShaderNodeVectorMath'); sub.operation = 'SUBTRACT'
    sub.inputs[1].default_value = (0.5, 0.5, 0)
    nt.links.new(fr.outputs[0], sub.inputs[0])
    # only XY distance
    sep = nt.nodes.new('ShaderNodeSeparateXYZ'); nt.links.new(sub.outputs[0], sep.inputs[0])
    comb = nt.nodes.new('ShaderNodeCombineXYZ')
    nt.links.new(sep.outputs['X'], comb.inputs['X']); nt.links.new(sep.outputs['Y'], comb.inputs['Y'])
    ln = nt.nodes.new('ShaderNodeVectorMath'); ln.operation = 'LENGTH'
    nt.links.new(comb.outputs[0], ln.inputs[0])
    st = nt.nodes.new('ShaderNodeMath'); st.operation = 'LESS_THAN'; st.inputs[1].default_value = dot
    nt.links.new(ln.outputs['Value'], st.inputs[0])
    mx = nt.nodes.new('ShaderNodeMix'); mx.data_type = 'RGBA'
    col = tuple(b.inputs['Base Color'].default_value)
    mx.inputs[6].default_value = col
    mx.inputs[7].default_value = (*ink, 1)
    fm = nt.nodes.new('ShaderNodeMath'); fm.operation = 'MULTIPLY'; fm.inputs[1].default_value = mix
    nt.links.new(st.outputs[0], fm.inputs[0])
    nt.links.new(fm.outputs[0], mx.inputs[0])
    nt.links.new(mx.outputs[2], b.inputs['Base Color'])
    return m


def image_mat(name, path, rough=0.35, coat=0.6, emit=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    b = nt.nodes['Principled BSDF']
    t = nt.nodes.new('ShaderNodeTexImage')
    t.image = bpy.data.images.load(path)
    t.extension = 'CLIP'
    nt.links.new(t.outputs['Color'], b.inputs['Base Color'])
    b.inputs['Roughness'].default_value = rough
    b.inputs['Coat Weight'].default_value = coat
    if emit:
        nt.links.new(t.outputs['Color'], b.inputs['Emission Color'])
        b.inputs['Emission Strength'].default_value = emit
    return m


# chalk soft-touch plastic, graphite, keys
CHALK = dotted(mat('chalk', hexc('#e9ebef'), 0.8, bump=(520, 0.14), spec=0.2), 0.13, 0.012, hexc('#c9cdd5'), 1.0)
TRAYM = dotted(mat('tray', hexc('#dde0e6'), 0.82, bump=(520, 0.14), spec=0.18), 0.13, 0.012, hexc('#c3c7d0'), 1.0)
GRAPH = dotted(mat('graphite', hexc('#26282d'), 0.5, bump=(1100, 0.08), spec=0.4), 0.08, 0.009, hexc('#3a3d44'), 1.0)
WELL = mat('well', hexc('#121315'), 0.35, bump=(1200, 0.03))
KEY = mat('key', hexc('#f8f8fa'), 0.42, coat=0.18, bump=(1400, 0.035), spec=0.3)
KEYD = mat('keyd', hexc('#2f3137'), 0.42, coat=0.3, bump=(1400, 0.04))
INK = mat('ink', hexc('#4b5563'), 0.6)
INKD = mat('inkd', hexc('#1f2329'), 0.6)
WHITEINK = mat('whiteink', hexc('#eceef2'), 0.5)
GREYINK = mat('greyink', hexc('#8a909b'), 0.55)
ALU = mat('alu', hexc('#c9ccd2'), 0.22, metal=1.0, aniso=0.85)
ALUD = mat('alud', hexc('#9aa0aa'), 0.35, metal=1.0)
CHROME = mat('chrome', hexc('#e6e8ec'), 0.12, metal=1.0)
RED = mat('red', hexc('#ff3b30'), 0.3, emit=hexc('#ff3b30'), estr=6)
LEDG = mat('ledg', hexc('#3ee08f'), 0.3, emit=hexc('#3ee08f'), estr=8)
LEDOFF = mat('ledoff', hexc('#b9bec7'), 0.3)
LEDY = mat('ledy', hexc('#ffb04a'), 0.3, emit=hexc('#ffb04a'), estr=7)
GLASS = mat('glass', (1, 1, 1), 0.02, trans=1.0, ior=1.5, spec=0.6)
LYR = mat('lyr', hexc('#c4e078'), 0.4, emit=hexc('#c4e078'), estr=3.0)
LYRD = mat('lyrd', hexc('#56603f'), 0.5, emit=hexc('#56603f'), estr=0.6)
GOLD = mat('gold', hexc('#f5b93b'), 0.35, emit=hexc('#f5b93b'), estr=1.5)
RIM = mat('rim', hexc('#f3f4f6'), 0.35, coat=0.4)

# the LCD: glass gradient + 6px grid, softly backlit
LCD = bpy.data.materials.new('lcd'); LCD.use_nodes = True
nt = LCD.node_tree; b = nt.nodes['Principled BSDF']
tc = nt.nodes.new('ShaderNodeTexCoord')
gr = nt.nodes.new('ShaderNodeTexGradient'); gr.gradient_type = 'SPHERICAL'
mp = nt.nodes.new('ShaderNodeMapping'); mp.inputs['Scale'].default_value = (0.25, 0.5, 1); mp.inputs['Location'].default_value = (0.1, 0.25, 0)
nt.links.new(tc.outputs['Object'], mp.inputs['Vector']); nt.links.new(mp.outputs['Vector'], gr.inputs['Vector'])
ramp = nt.nodes.new('ShaderNodeValToRGB')
ramp.color_ramp.elements[0].color = (*hexc('#a6b385'), 1)
ramp.color_ramp.elements[1].color = (*hexc('#d8e1b3'), 1)
nt.links.new(gr.outputs['Fac'], ramp.inputs['Fac'])
# grid
mp2 = nt.nodes.new('ShaderNodeMapping'); mp2.inputs['Scale'].default_value = (1 / 0.06, 1 / 0.06, 1)
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
b.inputs['Emission Strength'].default_value = 0.22
b.inputs['Roughness'].default_value = 0.55
LCDINK = mat('lcdink', hexc('#232b1b'), 0.6)
LCDGHOST = mat('lcdghost', hexc('#a3ae84'), 0.6)

# ───────────────────────────── geometry helpers
def link(o):
    sc.collection.objects.link(o)
    return o


def rbox(name, x0, y0, x1, y1, z0, z1, r, bev, m, segs=12):
    """Rounded box in UI px (x0..x1, y0..y1), z in world units; r = corner radius px, bev = edge bevel px."""
    ax, ay = P(x0, y0); bx, by = P(x1, y1)
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1)
    for v in bm.verts:
        v.co.x = ax if v.co.x < 0 else bx
        v.co.y = by if v.co.y < 0 else ay
        v.co.z = z0 if v.co.z < 0 else z1
    if r > 0:
        vert_edges = [e for e in bm.edges if abs(e.verts[0].co.x - e.verts[1].co.x) < 1e-6 and abs(e.verts[0].co.y - e.verts[1].co.y) < 1e-6]
        rr = min(r / 100, (x1 - x0) / 200 - 1e-4, (y1 - y0) / 200 - 1e-4)
        bmesh.ops.bevel(bm, geom=vert_edges, offset=rr, offset_type='OFFSET', segments=segs, profile=0.5, affect='EDGES', clamp_overlap=True)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me); bm.free()
    for p in me.polygons:
        p.use_smooth = True
    o = link(bpy.data.objects.new(name, me))
    o.data.materials.append(m)
    if bev > 0:
        md = o.modifiers.new('bev', 'BEVEL')
        md.width = bev / 100
        md.segments = 6
        md.limit_method = 'ANGLE'
        md.angle_limit = math.radians(50)
        md.harden_normals = True
    return o


def cyl(name, cx, cy, rad, z0, z1, m, bev=0, segs=96):
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=segs, radius1=rad / 100, radius2=rad / 100, depth=z1 - z0)
    me = bpy.data.meshes.new(name); bm.to_mesh(me); bm.free()
    for p in me.polygons:
        p.use_smooth = True
    o = link(bpy.data.objects.new(name, me))
    x, y = P(cx, cy)
    o.location = (x, y, (z0 + z1) / 2)
    o.data.materials.append(m)
    if bev:
        md = o.modifiers.new('bev', 'BEVEL'); md.width = bev / 100; md.segments = 6
        md.limit_method = 'ANGLE'; md.angle_limit = math.radians(50); md.harden_normals = True
    return o


def cut(target, cutter):
    md = target.modifiers.new('cut_' + cutter.name, 'BOOLEAN')
    md.operation = 'DIFFERENCE'
    md.solver = 'EXACT'
    md.object = cutter
    cutter.hide_render = True
    cutter.display_type = 'WIRE'
    return md


_fonts = {}


def font(name):
    if name not in _fonts:
        _fonts[name] = bpy.data.fonts.load(os.path.join(FONTS, name))
    return _fonts[name]


def text(body, x, y, size_px, fnt, m, z, align='LEFT', valign='TOP_BASELINE', extrude=0.0, spacing=1.0, rot=0):
    cu = bpy.data.curves.new('t', 'FONT')
    cu.body = body
    cu.font = font(fnt)
    cu.size = size_px / 100 * {'galmuri11.ttf': 1.45, 'dotgothic16.ttf': 1.1, 'pixelify_600.ttf': 1.2}.get(fnt, 1.0)
    cu.align_x = align
    cu.align_y = valign
    cu.extrude = extrude
    cu.space_character = spacing
    o = link(bpy.data.objects.new('t_' + body[:12], cu))
    xx, yy = P(x, y)
    o.location = (xx, yy, z)
    o.rotation_euler = (0, 0, math.radians(rot))
    o.data.materials.append(m)
    return o


ICONS = {
    'RADIO': [".........##.", ".......##...", ".....##.....", "############", "#..........#", "#.####..##.#", "#.####.#..##", "#.####.#..##", "#.####..##.#", "#..........#", "############"],
    'SEARCH': ["..#####.....", ".##...##....", "##.....##...", "#.......#...", "#.......#...", "#.......#...", "##.....##...", ".##...###...", "..#####.##..", ".........##.", "..........##", "...........#"],
    'CHAT': [".##########.", "############", "##........##", "##.##.##.###", "##........##", "############", ".##########.", "..###.......", "..##........", "..#........."],
    'LIBRARY': ["##.##..##...", "##.##..###..", "##.##...##..", "##.##...###.", "##.##....##.", "##.##....###", "##.##.....##", "##.##.....##", "############"],
    'USER': ["....####....", "...######...", "...######...", "...######...", "....####....", "............", "..########..", ".##########.", "############", "############"],
    'GEAR': [".....##.....", "..#..##..#..", ".##########.", "..########..", "..###..###..", "####....####", "####....####", "..###..###..", "..########..", ".##########.", "..#..##..#..", ".....##....."],
    'PLAY': ["##......", "####....", "######..", "########", "########", "######..", "####....", "##......"],
    'HEART': [".###...###.", "#...#.#...#", "#....#....#", "#.........#", ".#.......#.", "..#.....#..", "...#...#...", "....#.#....", ".....#....."],
    'INFO': [".##########.", "#..........#", "#....##....#", "#..........#", "#...###....#", "#....##....#", "#....##....#", "#....##....#", "#...####...#", "#..........#", ".##########."],
    'SAVE': ["##########..", "#..#####.##.", "#..#####..##", "#..##.##...#", "#..#####...#", "#..........#", "#.########.#", "#.#......#.#", "#.########.#", "#.#......#.#", "############"],
    'NOTE': ["...#######", "...#######", "...#.....#", "...#.....#", "...#.....#", "...#.....#", ".###...###", "####..####", "####..####", ".##....##."],
    'CHECK': [".........##", "........##.", ".......##..", "##....##...", ".##..##....", "..####.....", "...##......"],
}


def icon(name, cx, cy, px, m, z, h=0.006):
    rows = ICONS[name]
    w, hh = len(rows[0]), len(rows)
    bm = bmesh.new()
    s = px / 100
    x0, y0 = P(cx, cy)
    x0 -= w * s / 2; y0 += hh * s / 2
    for j, row in enumerate(rows):
        for i, c in enumerate(row):
            if c != '#':
                continue
            vs = [bm.verts.new((x0 + i * s + dx * s, y0 - j * s - dy * s, z)) for dx, dy in ((0, 0), (1, 0), (1, 1), (0, 1))]
            bm.faces.new(vs)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-6)
    f = bmesh.ops.extrude_face_region(bm, geom=bm.faces[:])
    for v in [e for e in f['geom'] if isinstance(e, bmesh.types.BMVert)]:
        v.co.z += h
    me = bpy.data.meshes.new('ic'); bm.to_mesh(me); bm.free()
    o = link(bpy.data.objects.new('ic_' + name, me))
    o.data.materials.append(m)
    return o


def plane(name, x0, y0, x1, y1, z, m, rot=0, r=0):
    o = rbox(name, x0, y0, x1, y1, z - 0.002, z, r, 0, m, segs=10)
    # UV from bounds for image textures
    me = o.data
    me.uv_layers.new()
    ax, ay = P(x0, y0); bx, by = P(x1, y1)
    for loop in me.loops:
        co = me.vertices[loop.vertex_index].co
        me.uv_layers.active.data[loop.index].uv = ((co.x - ax) / (bx - ax), (co.y - by) / (ay - by))
    if rot:
        cx, cy = (ax + bx) / 2, (ay + by) / 2
        me.transform(__import__('mathutils').Matrix.Translation((-cx, -cy, 0)))
        o.location = (cx, cy, 0)
        o.rotation_euler = (0, math.radians(rot), 0)
    return o


def key(name, x0, y0, x1, y1, r, down=False, m=None, z=0.0):
    """A physical key standing in a slightly bigger recessed well."""
    zt = z + (0.05 if down else 0.11)
    k = rbox(name, x0, y0, x1, y1, z - 0.05, zt, r, 3.2, m or KEY)
    return k, zt


# ───────────────────────────── the chassis
Z0 = 0.0
chassis = rbox('chassis', 0, 0, W, H, -0.4, Z0, 14, 0, CHALK)
cutters = []


def recess(name, x0, y0, x1, y1, r, depth):
    c = rbox('cut_' + name, x0, y0, x1, y1, -depth, 0.3, r, 0, CHALK)
    cut(chassis, c)
    floor = rbox('floor_' + name, x0 - 1, y0 - 1, x1 + 1, y1 + 1, -depth - 0.05, -depth, r, 0, TRAYM)
    return -depth


# nav rail tray
navz = recess('nav', 16, 16, 108, 984, 30, 0.07)
# centre bay (graphite insert sits in a recess with a 4 px seam)
recess('bay', 122, 16, 1166, 568, 36, 0.16)
bay = rbox('bay', 127, 21, 1161, 563, -0.5, -0.03, 32, 4, GRAPH)
# deck trays
recess('lcd', 126, 584, 1162, 748, 26, 0.09)

recess('grille', 826, 764, 950, 972, 22, 0.05)
recess('knobwell', 966, 764, 1162, 972, 30, 0.06)
recess('keys2', 126, 876, 812, 976, 28, 0.06)
# right column trays
recess('desc', 1180, 16, 1584, 474, 30, 0.07)
recess('chat', 1180, 490, 1584, 984, 30, 0.07)

# ───────────────────────────── nav keys
NAV = [('RADIO', 'ASOSIY'), ('SEARCH', 'QIDIRUV'), ('CHAT', 'CHAT'), ('LIBRARY', 'KUTUBXONA'), ('USER', 'PROFIL')]
y = 26
for i, (ic, lab) in enumerate(NAV):
    down = i == 0
    k, zt = key('nav%d' % i, 24, y, 100, y + 80, 20, down, z=navz)
    icon(ic, 62, y + 32, 2.6, INK if not down else INKD, zt)
    text(lab, 62, y + 66, 12.5, 'galmuri11.ttf', INK if not down else INKD, zt + 0.001, align='CENTER')
    # light pipe
    cyl('lp%d' % i, 89, y + 11, 3.2, zt - 0.01, zt + 0.004, LEDG if down else LEDOFF)
    y += 92
k, zt = key('gear', 24, 912, 100, 976, 20, z=navz)
icon('GEAR', 62, 944, 2.2, INK, zt)

# ───────────────────────────── the bay
BZ = -0.03


def bwell(name, x0, y0, x1, y1, r, depth):
    c = rbox('bcut_' + name, x0, y0, x1, y1, BZ - depth, BZ + 0.3, r, 0, WELL)
    md = bay.modifiers.new('cut_' + name, 'BOOLEAN'); md.operation = 'DIFFERENCE'; md.solver = 'EXACT'; md.object = c
    c.hide_render = True
    # keep the boolean before the bay's bevel
    bay.modifiers.move(len(bay.modifiers) - 1, 0)
    rbox('bfloor_' + name, x0 - 1, y0 - 1, x1 + 1, y1 + 1, BZ - depth - 0.05, BZ - depth, r, 0, WELL)
    return BZ - depth

# logo (SVG) and title
try:
    bpy.ops.import_curve.svg(filepath=os.path.join(HERE, '..', 'logo.svg'))
    logo = [o for o in bpy.context.scene.objects if o.type == 'CURVE' and o.name.startswith('Curve')]
    for o in logo:
        o.data.materials.clear(); o.data.materials.append(WHITEINK)
        o.data.extrude = 0.002
        # SVG import size: 1 svg unit = 1 cm? scale to ~30 px tall
        bbox = [o.matrix_world @ Vector(c) for c in o.bound_box]
        hgt = max(v.y for v in bbox) - min(v.y for v in bbox)
        s = 0.34 / max(hgt, 1e-6)
        o.scale = (o.scale.x * s, o.scale.y * s, 1)
        bpy.context.view_layer.update()
        bbox = [o.matrix_world @ Vector(c) for c in o.bound_box]
        mnx, mxy = min(v.x for v in bbox), max(v.y for v in bbox)
        tx, ty = P(152, 46)
        o.location.x += tx - mnx; o.location.y += ty - mxy; o.location.z = BZ + 0.002
except Exception as e:
    print('logo failed', e)
text('Kawaii Radio', 196, 70, 23, 'inter_800.ttf', WHITEINK, BZ + 0.001)
text('KR-01 · J-POP FM · 105.7', 197, 93, 13, 'galmuri11.ttf', GREYINK, BZ + 0.001, spacing=1.15)

# band selector: a slide switch in a dark slot
bwell('slot', 640, 40, 1138, 108, 22, 0.08)
sk, szt = key('slider', 646, 46, 900, 102, 17, m=KEY, z=BZ - 0.08)
text('RADIO', 668, 72, 17, 'galmuri11.ttf', INKD, szt + 0.001, spacing=1.1)
text('jonli efir', 668, 93, 15, 'dotgothic16.ttf', INK, szt + 0.001)
for gx in (864, 870, 876):
    rbox('grip', gx, 58, gx + 2.4, 90, szt - 0.004, szt + 0.003, 1, 0, LEDOFF)
cyl('sled', 846, 74, 4, szt - 0.01, szt + 0.003, RED)
text('››› O\'Z TANLOVIM', 1020, 79, 14, 'galmuri11.ttf', GREYINK, BZ - 0.079, align='CENTER', spacing=1.1)

# station keys (the phone's J-Pop / OST / Riot tabs, as preset buttons with LED windows)
STATIONS = ['J-POP', 'OST', 'ANIME', 'RIOT']
x = 150
for i, s in enumerate(STATIONS):
    on = i == 0
    bwell('stw%d' % i, x - 4, 122, x + 124, 182, 18, 0.06)
    k, zt = key('st%d' % i, x, 126, x + 120, 178, 15, on, m=KEYD, z=BZ - 0.06)
    text(s, x + 60, 163, 18, 'galmuri11.ttf', WHITEINK if on else GREYINK, zt + 0.001, align='CENTER', spacing=1.12)
    rbox('stled%d' % i, x + 44, 133, x + 76, 138, zt - 0.01, zt + 0.003, 2, 0, LEDG if on else mat('ledoffd%d' % i, hexc('#43464d'), 0.4))
    x += 134
# on-air block
cyl('onair', 760, 152, 5, BZ - 0.005, BZ + 0.006, RED)
text('EFIRDA · 214 TINGLOVCHI', 776, 159, 16, 'galmuri11.ttf', GREYINK, BZ + 0.001, spacing=1.12)

# cover carousel
covers = ['eve-tokyo-ghetto-mv8di1fh', 'fujii-kaze-matsuri-mv84iqkb', 'jpop-yoasobi-yoru-ni-kake-ru-3dptxh-mvdsi3h0',
          'jpop-kenshi-yonezu-kick-back-3kheep-mvdvifuv', 'kenshi-yonezu-lemon-mv84iyp8', 'creepy-nuts-bling-bang-bang-born-mv84iew9', 'yoasobi-mv8ditp6-mv8ditp6']
cx0, cy0 = 644, 330
for i, c in enumerate(covers):
    d = i - 3
    m = image_mat('cv%d' % i, os.path.join(TEX, c + '.jpg'), emit=0.0 if d else 0.0)
    if d == 0:
        rbox('cvrim', cx0 - 124, cy0 - 124, cx0 + 124, cy0 + 124, BZ, BZ + 0.18, 26, 3, RIM)
        plane('cv', cx0 - 115, cy0 - 115, cx0 + 115, cy0 + 115, BZ + 0.182, m, r=20)
        cyl('ok', cx0 - 92, cy0 - 92, 17, BZ + 0.18, BZ + 0.24, KEY, bev=3)
        icon('CHECK', cx0 - 92, cy0 - 92, 1.9, INKD, BZ + 0.24)
    else:
        side = 1 if d > 0 else -1
        a = abs(d)
        size = [0, 200, 176, 150][a]
        off = [0, 246, 400, 520][a]
        cx = cx0 + side * off
        o = plane('cv%d' % i, cx - size / 2, cy0 - size / 2, cx + size / 2, cy0 + size / 2, 0, m, r=16, rot=side * (24 + a * 9))
        o.location.z = BZ + 0.6 - a * 0.1
        # darken the far ones a little
        bsdf = m.node_tree.nodes['Principled BSDF']
        bsdf.inputs['Coat Weight'].default_value = 0.8
        if a > 1:
            nt = m.node_tree
            hsv = nt.nodes.new('ShaderNodeHueSaturation'); hsv.inputs['Value'].default_value = 1 - 0.22 * (a - 1)
            img = [n for n in nt.nodes if n.type == 'TEX_IMAGE'][0]
            nt.links.new(img.outputs['Color'], hsv.inputs['Color']); nt.links.new(hsv.outputs['Color'], bsdf.inputs['Base Color'])

# lyrics well
bwell('lyw', 150, 470, 1138, 546, 20, 0.07)
cyl('lydot', 172, 488, 3.5, BZ - 0.07, BZ - 0.064, GREYINK)
text('KICK BACK', 184, 493, 14, 'dotgothic16.ttf', WHITEINK, BZ - 0.069)
text('Kenshi Yonezu', 272, 493, 14, 'dotgothic16.ttf', GREYINK, BZ - 0.069)
text('ROM', 1118, 494, 14, 'galmuri11.ttf', GREYINK, BZ - 0.069, align='RIGHT')
text("Qo'shiq matni shu yerda", 172, 532, 31, 'vt323.ttf', LYR, BZ - 0.068)
text("qator-qator yonadi", 510, 532, 31, 'vt323.ttf', LYRD, BZ - 0.068)

# ───────────────────────────── deck: the big LCD under glass
LZ = -0.09
rbox('lcdframe', 132, 590, 1156, 742, LZ, LZ + 0.02, 22, 1.5, mat('lcdb', hexc('#363d2e'), 0.5))
rbox('lcd', 137, 595, 1151, 737, LZ, LZ + 0.024, 18, 0, LCD)
LT = LZ + 0.0245
icon('NOTE', 166, 616, 1.5, LCDINK, LT)
text('KICK BACK · Kenshi Yonezu', 182, 624, 20, 'dotgothic16.ttf', LCDINK, LT)
text('8', 990, 632, 40, 'vt323.ttf', LCDGHOST, LT)
text('1:42', 1010, 632, 40, 'vt323.ttf', LCDINK, LT)
text('/ 3:13', 1134, 632, 22, 'vt323.ttf', LCDGHOST, LT, align='RIGHT')
# waveform
import random
random.seed(7)
n = 150
for i in range(n):
    x0 = 160 + i * (970 / n)
    hgt = 8 + abs(math.sin(i * .43) * 30 + math.sin(i * .13) * 16 + random.random() * 8)
    hgt = min(hgt, 52)
    mm = LCDINK if i < 80 else LCDGHOST
    rbox('wv%d' % i, x0, 666 - hgt / 2, x0 + 3.6, 666 + hgt / 2, LT - 0.001, LT + 0.0015, 0, 0, mm)
rbox('head', 160 + 80 * (970 / n), 636, 163 + 80 * (970 / n), 696, LT - 0.001, LT + 0.002, 0, 0, LCDINK)
rbox('lcdline', 160, 704, 1130, 705, LT - 0.001, LT + 0.0012, 0, 0, LCDGHOST)
text('UZ o   ROM *   JP o      J-POP · 172 BPM · ENERGIYA 92%', 162, 729, 22, 'vt323.ttf', LCDINK, LT)
text('ON AIR *', 1130, 729, 22, 'vt323.ttf', LCDINK, LT, align='RIGHT')
# the glass over it
glass = rbox('glass', 135, 593, 1153, 739, LZ + 0.05, LZ + 0.065, 20, 1, GLASS)

# ───────────────────────────── deck controls
# play / heart: round keys
DZ = 0.0
for nm, cx, ic in (('play', 178, 'PLAY'), ('heart', 766, 'HEART')):
    cyl(nm + 'w', cx, 814, 52, -0.06, 0.0005, TRAYM)
    k = cyl(nm, cx, 814, 46, -0.06, 0.12, KEY, bev=6)
    icon(ic, cx + (3 if ic == 'PLAY' else 0), 814, 3.0, INK, 0.12)

# tuner drum: a wheel seen from its side (axis along the screen's Y), sunk in a deep slot
c = rbox('cut_drumdeep', 248, 770, 696, 858, -2.6, 0.3, 20, 0, CHALK); cut(chassis, c)
rbox('drumfloor', 246, 768, 698, 860, -2.7, -2.6, 20, 0, WELL)
R = 2.18
DRUM = mat('drumm', hexc('#eef0f3'), 0.32, coat=0.35, emit=hexc('#eef0f3'), estr=0.4)
bm = bmesh.new()
bmesh.ops.create_cone(bm, cap_ends=True, segments=256, radius1=R, radius2=R, depth=0.82)
me = bpy.data.meshes.new('drum'); bm.to_mesh(me); bm.free()
for p_ in me.polygons:
    p_.use_smooth = True
dr = link(bpy.data.objects.new('drum', me))
x, y = P(472, 814)
dr.location = (x, y, -R + 0.02)
dr.rotation_euler = (math.radians(90), 0, 0)
dr.data.materials.append(DRUM)
for i in range(-40, 41):
    th = math.radians(i * 2.2)
    if abs(th) > math.radians(84):
        continue
    long = i % 5 == 0
    t = rbox('tick', 0, 0, 1, 1, 0, 1, 0, 0, INK)
    t.data.transform(__import__('mathutils').Matrix.Translation((-t.location.x, 0, 0)))
    me_ = t.data
    for v in me_.vertices:
        v.co = Vector((0, 0, 0))
    bm = bmesh.new(); bmesh.ops.create_cube(bm, size=1); bm.to_mesh(me_); bm.free()
    t.scale = (0.022 if long else 0.012, 0.46 if long else 0.30, 0.006)
    t.location = (x + R * math.sin(th), y, -R + 0.02 + R * math.cos(th) + 0.002)
    t.rotation_euler = (0, th, 0)

# fixed red needle and a glass window over the drum
rbox('needle', 466, 778, 478, 850, 0.05, 0.08, 6, 1.5, RED)
rbox('drumglass', 250, 772, 694, 856, 0.03, 0.045, 18, 1, GLASS)

# station description keys
for nm, x0, x1, ic, lab in (('tavsif', 132, 464, 'INFO', 'TAVSIF'), ('saqlash', 474, 806, 'SAVE', 'SAQLASH')):
    k, zt = key(nm, x0, 882, x1, 970, 24, z=-0.06)
    icon(ic, (x0 + x1) / 2 - 52, 926, 2.2, INK, zt)
    text(lab, (x0 + x1) / 2 - 30, 935, 20, 'galmuri11.ttf', INK, zt + 0.001, spacing=1.15)

# speaker grille: perforated aluminium plate
gp = rbox('grillep', 832, 770, 944, 966, -0.05, -0.02, 18, 1.5, ALUD)
for j in range(24):
    for i in range(13):
        xx = 842 + i * 8 + (4 if j % 2 else 0)
        yy = 780 + j * 7.9
        if xx > 936 or yy > 958:
            continue
        cyl('h', xx, yy, 2.2, -0.0205, -0.0195, WELL, segs=12)
text('SPK', 888, 992, 11, 'galmuri11.ttf', INK, 0.001, align='CENTER')

# volume knob: knurled aluminium with an LED ring
kx, ky = 1064, 868
ring = 34
for i in range(31):
    a = math.radians(-225 + i * 270 / 30)
    on = i < 22
    m = (LEDY if i > 19 else LEDG) if on else LEDOFF
    cyl('rl%d' % i, kx + math.cos(a) * 88, ky - math.sin(a) * 88, 3.2, -0.062, -0.054, m, segs=16)
# knurl: 120 thin ribs around a cylinder
body = cyl('knob', kx, ky, 72, -0.06, 0.34, ALU, bev=2, segs=128)
for i in range(120):
    a = i / 120 * math.tau
    rib = rbox('rib', 0, 0, 3, 2, 0, 1, 0, 0, ALUD)
    rib.location = (P(kx, ky)[0] + math.cos(a) * 0.72, P(kx, ky)[1] + math.sin(a) * 0.72, 0)
    rib.dimensions = (0.028, 0.02, 0.36)
    rib.location.z = -0.05
    rib.rotation_euler = (0, 0, a)
cap = cyl('kcap', kx, ky, 58, 0.30, 0.37, ALU, bev=2.5, segs=128)
a = math.radians(90 - 58)
cyl('kdot', kx + math.cos(a) * 42, ky - math.sin(a) * 42, 6.5, 0.36, 0.378, RED, bev=1.5)
text('OVOZ', kx - 4, 994, 11, 'galmuri11.ttf', INK, 0.001, align='RIGHT')
text('72', kx + 4, 994, 16, 'vt323.ttf', INKD, 0.001)

# ───────────────────────────── right: song description panel
RZ = -0.07
cvk = image_mat('desccv', os.path.join(TEX, 'jpop-kenshi-yonezu-kick-back-3kheep-mvdvifuv.jpg'))
rbox('dcrim', 1200, 36, 1330, 166, RZ, RZ + 0.12, 20, 3, RIM)
plane('dcv', 1206, 42, 1324, 160, RZ + 0.122, cvk, r=15)
text("QO'SHIQ HAQIDA", 1346, 58, 13, 'galmuri11.ttf', INK, RZ + 0.001, spacing=1.2)
text('KICK BACK', 1346, 92, 30, 'dotgothic16.ttf', INKD, RZ + 0.001)
text('Kenshi Yonezu', 1347, 116, 16, 'dotgothic16.ttf', INK, RZ + 0.001)
xx = 1346
for lab, wdt in (('J-POP', 70), ('172 BPM', 92), ('92%', 58)):
    k, zt = key('chip', xx, 132, xx + wdt, 160, 14, z=RZ)
    text(lab, xx + wdt / 2, 152, 13, 'galmuri11.ttf', INK, zt + 0.001, align='CENTER', spacing=1.1)
    xx += wdt + 8
# description LCD
rbox('dframe', 1196, 186, 1568, 458, RZ, RZ + 0.02, 20, 1.5, mat('lcdb2', hexc('#363d2e'), 0.5))
rbox('dlcd', 1201, 191, 1563, 453, RZ, RZ + 0.024, 16, 0, LCD)
DT = RZ + 0.0245
lines = ["Kenshi Yonezu «Chainsaw Man»", "animesi uchun yozgan opening.", "2022-yil oktyabrida chiqqan,", "keyin AQShda «Gold» olgan", "birinchi yaponcha qo'shiq", "bo'ldi. Arra ovozi, keskin", "ritm va baqiriqli naqarot —", "anime ruhining o'zi."]
for i, l in enumerate(lines):
    text(l, 1222, 228 + i * 27, 22, 'vt323.ttf', LCDINK, DT)
rbox('dglass', 1199, 189, 1565, 455, RZ + 0.05, RZ + 0.062, 18, 1, GLASS)

# ───────────────────────────── right: chat
CZ = -0.07
text('CHAT', 1202, 538, 32, 'galmuri11.ttf', INKD, CZ + 0.001, spacing=1.1)
text('214 onlayn · 31 radioda', 1300, 534, 13, 'dotgothic16.ttf', INK, CZ + 0.001)
cyl('live', 1524, 528, 4, CZ, CZ + 0.008, LEDG)
text('LIVE', 1534, 534, 12, 'galmuri11.ttf', mat('greenink', hexc('#1f8a55'), 0.5), CZ + 0.001)
# pinned
k, zt = key('pin', 1196, 552, 1568, 600, 16, z=CZ)
rbox('pinbar', 1210, 560, 1213.5, 592, zt - 0.002, zt + 0.003, 1, 0, GOLD)
text("QADALGAN · SYNTX", 1224, 573, 11, 'galmuri11.ttf', INK, zt + 0.001, spacing=1.2)
text("!! Ilovada yangimisiz?", 1224, 592, 15, 'dotgothic16.ttf', INKD, zt + 0.001)
# bubbles
BUBS = [
    ('fujii-kaze-matsuri-mv84iqkb', 'sakura', None, ["salom hammaga :)", "bugun nima eshitamiz?"], 618, 692),
    ('eve-underdog-mv84in6q', 'syntx', ('PATROL', '#3b6fe0'), ["♪ Tokyo Ghetto · Eve"], 706, 768),
    ('ado-odo-mv84hade', 'Last veRum', ('GOLD II', '#e8b931'), ["wassup qoravoyla <3"], 782, 844),
    ('kenshi-yonezu-iris-out-mv84ixgy', 'z.ortiqov', ('EMERALD', '#34c08f'), ["KICK BACK efirda!!"], 858, 914),
]
for img, who, badge, msg, y0, y1 in BUBS:
    am = image_mat('av_' + who, os.path.join(TEX, img + '.jpg'), coat=0.3)
    cyl('avr', 1218, y1 - 18, 19, CZ, CZ + 0.08, RIM, bev=2)
    av = plane('av', 1201, y1 - 35, 1235, y1 - 1, CZ + 0.0815, am, r=17)
    wdt = 300 if len(msg) > 1 or len(msg[0]) > 18 else 240
    k, zt = key('bub', 1246, y0, 1246 + wdt, y1, 18, z=CZ)
    text(who, 1262, y0 + 21, 13, 'dotgothic16.ttf', INK, zt + 0.001)
    if badge:
        bw = 9 * len(badge[0]) + 10
        bx = 1262 + 9 * len(who) + 10
        rbox('bdg', bx, y0 + 8, bx + bw, y0 + 24, zt, zt + 0.004, 5, 0, mat('bd' + who, hexc(badge[1]), 0.4))
        text(badge[0], bx + bw / 2, y0 + 21, 10, 'pixelify_600.ttf', WHITEINK if badge[0] != 'GOLD II' else INKD, zt + 0.0045, align='CENTER')
    for i, l in enumerate(msg):
        text(l, 1262, y0 + 44 + i * 19, 15, 'dotgothic16.ttf', INKD, zt + 0.001)
# compose: radio key, LCD input, smiley key
k, zt = key('cmpk', 1196, 932, 1248, 976, 14, z=CZ)
icon('RADIO', 1222, 954, 1.7, INK, zt)
rbox('inframe', 1256, 930, 1508, 978, CZ, CZ + 0.02, 14, 1.5, mat('lcdb3', hexc('#363d2e'), 0.5))
rbox('inlcd', 1260, 934, 1504, 974, CZ, CZ + 0.024, 11, 0, LCD)
text('XABAR YOZING', 1276, 961, 19, 'vt323.ttf', LCDGHOST, CZ + 0.0245, spacing=1.15)
rbox('caret', 1392, 942, 1394.5, 966, CZ + 0.0245, CZ + 0.026, 0, 0, LCDINK)
k, zt = key('smile', 1516, 932, 1568, 976, 14, z=CZ)
text(':)', 1542, 962, 20, 'dotgothic16.ttf', INK, zt + 0.001, align='CENTER')

# screws on the chassis
for sx, sy in ((1172, 8), (1172, 992), (8, 500), (1592, 500)):
    pass

# ───────────────────────────── compositor: bloom for LEDs, backlight
sc.use_nodes = True
tree = sc.node_tree
rl = tree.nodes.get('Render Layers') or tree.nodes.new('CompositorNodeRLayers')
comp = tree.nodes.get('Composite') or tree.nodes.new('CompositorNodeComposite')
try:
    gl = tree.nodes.new('CompositorNodeGlare')
    try:
        gl.glare_type = 'FOG_GLOW'
    except Exception:
        gl.glare_type = 'BLOOM'
    for k_, v_ in (('threshold', 1.0), ('size', 7), ('quality', 'HIGH'), ('mix', -0.55)):
        try:
            setattr(gl, k_, v_)
        except Exception:
            pass
    for nm, v_ in (('Threshold', 1.0), ('Strength', 0.6), ('Size', 0.6)):
        if nm in gl.inputs:
            try:
                gl.inputs[nm].default_value = v_
            except Exception:
                pass
    tree.links.new(rl.outputs['Image'], gl.inputs['Image'])
    tree.links.new(gl.outputs['Image'], comp.inputs['Image'])
except Exception as e:
    print('glare failed', e)
    tree.links.new(rl.outputs['Image'], comp.inputs['Image'])

sc.render.filepath = os.path.abspath(OUT)
sc.render.image_settings.file_format = 'PNG'
bpy.ops.wm.save_as_mainfile(filepath=os.path.join(HERE, 'kr01.blend'))
bpy.ops.render.render(write_still=True)
print('WROTE', sc.render.filepath)
