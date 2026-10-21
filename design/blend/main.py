"""
KR-01 DESK main screen, 1920 x 1080, orthographic (1 unit = 100 px).

  blender -b -P main.py -- --theme light|dark --out file.png [--print 0..1] [--samples N] [--region x0 y0 x1 y1]
--print: how far the TAVSIF receipt is out (0 = inside the printer, 1 = fully printed).
--region: render only that part (for the receipt animation).
"""
import bpy, bmesh, math, os, sys, random
from mathutils import Vector, Matrix
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib
from lib import P, hexc, W, H

argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []


def arg(k, d=None, f=str):
    return f(argv[argv.index(k) + 1]) if k in argv else d


THEME = arg('--theme', 'light')
PRINT = arg('--print', 1.0, float)
SCALE = arg('--scale', 1, int)          # 2 = a 3840 x 2160 render of the same 1920 x 1080 layout
C = lib.Ctx(THEME, W * SCALE, H * SCALE, arg('--samples', 256, int))
C.sc.camera.data.ortho_scale = W / 100
T = C.T
C.studio()
sc = C.sc
DARK = THEME == 'dark'
MI, IN, IN6, DOT, VT, GAL = 'mplus_r_xb.ttf', 'mplus_r_md.ttf', 'mplus_r_bd.ttf', 'dotgothic16.ttf', 'vt323.ttf', 'galmuri11.ttf'
DELA = 'dela.ttf'
# app assets: --app leaves out everything the app draws live (texts, LCDs, covers, receipt, chat);
# --down renders every key pressed and every LED lit, so the app can cut the pressed/lit look out of it
APP = '--app' in argv
DOWN = '--down' in argv
INK, INKD, INKS = C.INK, C.INKD, C.INKS

# ───────────────────────────── chassis and its recesses
chassis = C.rbox('chassis', -2, -2, W + 2, H + 2, -0.4, 0.0, 0 if '--app' in argv else 14, 0, C.CHALK)


def recess(name, x0, y0, x1, y1, r, depth, floor=True):
    c = C.rbox('cut_' + name, x0, y0, x1, y1, -depth, 0.3, r, 0, C.CHALK)
    C.cut(chassis, c)
    if floor:
        C.rbox('floor_' + name, x0 - 1, y0 - 1, x1 + 1, y1 + 1, -depth - 0.05, -depth, r, 0, C.TRAY)
    return -depth


def round_recess(name, cx, cy, rad, depth):
    c = C.cyl('cut_' + name, cx, cy, rad, -depth, 0.3, C.CHALK, segs=128)
    C.cut(chassis, c)
    C.cyl('floor_' + name, cx, cy, rad + 1, -depth - 0.05, -depth, C.TRAY, segs=128)
    return -depth


navz = recess('nav', 16, 16, 112, 1064, 30, 0.07)
recess('bay', 122, 16, 1398, 650, 36, 0.16)
bay = C.rbox('bay', 127, 21, 1393, 645, -0.5, -0.03, 32, 4, C.GRAPH)
recess('lcd', 122, 666, 1398, 840, 26, 0.09)
recess('grille', 880, 858, 1016, 1062, 22, 0.05)
recess('keys2', 122, 970, 770, 1064, 28, 0.06)
recess('pn', 1030, 856, 1102, 1062, 24, 0.06)
kwz = round_recess('knob', 1250, 958, 100, 0.06)
recess('print', 1410, 16, 1904, 562, 30, 0.07)
recess('chat', 1410, 578, 1904, 1064, 30, 0.07)

# ───────────────────────────── nav
NAV = [('RADIO', 'ASOSIY'), ('SEARCH', 'QIDIRUV'), ('CHAT', 'CHAT'), ('LIBRARY', 'KUTUBXONA'), ('USER', 'PROFIL')]
y = 28
for i, (ic, lab) in enumerate(NAV):
    down = DOWN if APP else i == 0
    k, zt = C.key('nav%d' % i, 26, y, 102, y + 84, 20, down, z=navz)
    C.icon(ic, 64, y + 34, 2.6, INKD if down else INK, zt)
    C.text(lab, 64, y + 72, 10.5, GAL, INKD if down else INK, zt + 0.001, align='CENTER', spacing=1.05)
    C.cyl('lp%d' % i, 91, y + 11, 3.2, zt - 0.01, zt + 0.004, C.LEDG if down else C.LEDOFF)
    y += 96
# a vertical level meter of real LEDs
for j in range(18):
    yy = 548 + j * 22
    lit = DOWN if APP else j >= 6
    m = (C.LED_HOT if j < 9 else C.LED_ON) if lit else (C.LED_DIMR if j < 9 else C.LED_DIM)
    C.cyl('mb%d' % j, 64, yy, 7.5, navz - 0.01, navz + 0.012, C.BEZEL, segs=32)
    C.dome('md%d' % j, 64, yy, 5.6, navz + 0.012, m)
C.text('LEVEL', 64, 962, 10, MI, INKS, navz + 0.001, align='CENTER')
k, zt = C.key('gear', 26, 984, 102, 1052, 20, APP and DOWN, z=navz)
C.icon('GEAR', 64, 1018, 2.2, INK, zt)

# ───────────────────────────── the bay
BZ = -0.03


def bwell(name, x0, y0, x1, y1, r, depth, m=None):
    c = C.rbox('bcut_' + name, x0, y0, x1, y1, BZ - depth, BZ + 0.3, r, 0, C.WELL)
    C.cut(bay, c, first=True)
    C.rbox('bfloor_' + name, x0 - 1, y0 - 1, x1 + 1, y1 + 1, BZ - depth - 0.05, BZ - depth, r, 0, m or C.WELL)
    return BZ - depth


# logo
try:
    bpy.ops.import_curve.svg(filepath=os.path.join(lib.HERE, '..', 'logo.svg'))
    for o in [o for o in sc.objects if o.type == 'CURVE' and o.name.startswith('Curve')]:
        o.data.materials.clear(); o.data.materials.append(C.WHITE)
        bpy.context.view_layer.update()
        bb = [o.matrix_world @ Vector(c) for c in o.bound_box]
        s = 0.36 / max(max(v.y for v in bb) - min(v.y for v in bb), 1e-6)
        o.scale = (o.scale.x * s, o.scale.y * s, 1)
        bpy.context.view_layer.update()
        bb = [o.matrix_world @ Vector(c) for c in o.bound_box]
        tx, ty = P(156, 40)
        o.location.x += tx - min(v.x for v in bb); o.location.y += ty - max(v.y for v in bb); o.location.z = BZ + 0.002
except Exception as e:
    print('logo failed', e)
C.text('KAWAII RADIO', 202, 66, 27, DELA, C.WHITE, BZ + 0.001, spacing=1.06)
C.text('KR-01 DESK  ·  J-POP FM  ·  105.7', 203, 90, 11.5, MI, C.GREY, BZ + 0.001, spacing=1.1)

# band selector: slide switch
sz = bwell('slot', 836, 34, 1368, 102, 22, 0.08)
SLM = C.KEY if not DARK else C.mat('slk', hexc('#e9ebef'), 0.42, coat=0.2)
SLINK = C.DARKINK
if APP and DOWN:   # the slider on the right: O'Z TANLOVIM
    sk, szt = C.key('slider', 1100, 40, 1362, 96, 17, m=SLM, z=sz)
    C.text("O'Z TANLOVIM", 1146, 68, 16, MI, SLINK, szt + 0.001, spacing=1.08)
    C.text("istalgan qo'shiq", 1147, 89, 15, IN, C.mat('slink2', hexc('#6b7280'), 0.6), szt + 0.001)
    for gx in (1112, 1119, 1126):
        C.rbox('grip', gx, 52, gx + 2.6, 84, szt - 0.004, szt + 0.003, 1, 0, C.mat('gripm', hexc('#c3c7d0'), 0.5))
    C.dome('sled', 1336, 68, 5, szt, C.LED_ON)
    C.text("‹‹‹  RADIO", 968, 76, 14.5, MI, C.GREY, sz + 0.001, align='CENTER', spacing=1.08)
else:
    sk, szt = C.key('slider', 842, 40, 1104, 96, 17, m=SLM, z=sz)
    C.text('RADIO', 866, 68, 16, MI, SLINK, szt + 0.001, spacing=1.08)
    C.text('jonli efir', 867, 89, 15, IN, C.mat('slink2', hexc('#6b7280'), 0.6), szt + 0.001)
    for gx in (1066, 1073, 1080):
        C.rbox('grip', gx, 52, gx + 2.6, 84, szt - 0.004, szt + 0.003, 1, 0, C.mat('gripm', hexc('#c3c7d0'), 0.5))
    C.dome('sled', 1046, 68, 5, szt, C.LED_HOT)
    C.text("O'Z TANLOVIM  ›››", 1236, 76, 14.5, MI, C.GREY, sz + 0.001, align='CENTER', spacing=1.08)

# station keys with LED windows
x = 156
for i, s_ in enumerate(['J-POP', 'OST', 'RIOT']):
    on = DOWN if APP else i == 0
    wz = bwell('stw%d' % i, x - 4, 114, x + 204, 176, 18, 0.06)
    k, zt = C.key('st%d' % i, x, 118, x + 200, 172, 15, on, m=C.KEYS, z=wz)
    C.text(s_, x + 100, 157, 16, MI, C.WHITE if on else C.GREY, zt + 0.001, align='CENTER', spacing=1.08)
    C.rbox('stl%d' % i, x + 85, 125, x + 115, 130, zt - 0.01, zt + 0.003, 2, 0, C.LEDG if on else C.mat('ledoffd', hexc('#3e4148'), 0.4))
    x += 216
C.cyl('oab', 1102, 145, 9, BZ - 0.01, BZ + 0.012, C.BEZEL, segs=32)
C.dome('oa', 1102, 145, 6.5, BZ + 0.012, C.LED_HOT if (DOWN or not APP) else C.LED_DIMR)
C.text('ON AIR', 1122, 152, 13.5, MI, C.mat('oaink', hexc('#ff8a7a'), 0.5), BZ + 0.001, spacing=1.1)
C.text('214 TINGLOVCHI', 1366, 152, 13.5, MI, C.GREY, BZ + 0.001, align='RIGHT', spacing=1.08)

# the cover window: a flat trivision panel. 24 three-sided slats, each printed with a slice of three covers;
# changing the track turns them 120° in a wave (left to right, a little bounce). Seen straight on, no perspective.
TV = arg('--tv', 0.0, float)          # 0 = the current cover, 1 = the next one
TV_COVERS = ['jpop-kenshi-yonezu-kick-back-3kheep-mvdvifuv', 'jpop-yoasobi-yoru-ni-kake-ru-3dptxh-mvdsi3h0', 'kenshi-yonezu-lemon-mv84iyp8']
TX0, TY0, TX1, TY1 = 156, 196, 592, 628
tz = bwell('tv', TX0 - 6, TY0 - 6, TX1 + 6, TY1 + 6, 20, 0.16)
C.rbox('tvlip', TX0 - 6, TY0 - 6, TX1 + 6, TY1 + 6, tz, tz + 0.02, 20, 2, C.ALU)
C.rbox('tvback', TX0, TY0, TX1, TY1, tz + 0.02, tz + 0.025, 16, 0, C.WELL)


def slat_mat(k):
    m = bpy.data.materials.new('tvc%d' % k); m.use_nodes = True
    nt = m.node_tree; bsdf = nt.nodes['Principled BSDF']
    t = nt.nodes.new('ShaderNodeTexImage'); t.image = bpy.data.images.load(os.path.join(lib.TEX, TV_COVERS[k] + '.jpg')); t.extension = 'EXTEND'
    nt.links.new(t.outputs['Color'], bsdf.inputs['Base Color'])
    bsdf.inputs['Roughness'].default_value = 0.42; bsdf.inputs['Coat Weight'].default_value = 0.35; bsdf.inputs['Coat Roughness'].default_value = 0.25
    # printed on fine-ribbed aluminium: a whisper of vertical brushing
    n = nt.nodes.new('ShaderNodeTexNoise'); n.inputs['Scale'].default_value = 900; n.inputs['Detail'].default_value = 4
    mp = nt.nodes.new('ShaderNodeMapping'); mp.inputs['Scale'].default_value = (1, 0.02, 1)
    tc = nt.nodes.new('ShaderNodeTexCoord'); nt.links.new(tc.outputs['Object'], mp.inputs['Vector']); nt.links.new(mp.outputs['Vector'], n.inputs['Vector'])
    bp = nt.nodes.new('ShaderNodeBump'); bp.inputs['Strength'].default_value = 0.08; bp.inputs['Distance'].default_value = 0.01
    nt.links.new(n.outputs['Fac'], bp.inputs['Height']); nt.links.new(bp.outputs['Normal'], bsdf.inputs['Normal'])
    return m


TVM = [slat_mat(k) for k in range(3)]
NS = 24
sw = (TX1 - TX0) / 100 / NS
sr = sw / math.sqrt(3) * 1.0
x0w, y0w = P(TX0, TY0); x1w, y1w = P(TX1, TY1)
SLATS = []
for i in range(NS):
    bm = bmesh.new(); uv = bm.loops.layers.uv.new()
    # triangle in XZ: one face pointing up (+Z, to the camera)
    pts = [Vector((math.cos(math.radians(a)) * sr, math.sin(math.radians(a)) * sr)) for a in (-30, 90, 210)]
    pts = [Vector((math.cos(math.radians(a)) * sr, math.sin(math.radians(a)) * sr)) for a in (30, 150, 270)]
    hh = (y0w - y1w)
    near = [bm.verts.new((p.x, -hh / 2, p.y)) for p in pts]
    far = [bm.verts.new((p.x, hh / 2, p.y)) for p in pts]
    for k in range(3):
        a_, b_ = k, (k + 1) % 3
        f = bm.faces.new((near[a_], near[b_], far[b_], far[a_]))
        n = ((pts[a_] + pts[b_]) / 2)
        ang = math.degrees(math.atan2(n.y, n.x)) % 360          # 90 = facing the camera
        kk = min(((90, 0), (330, 1), (210, 2)), key=lambda v: abs(((ang - v[0] + 180) % 360) - 180))[1]
        f.material_index = kk
        u0, u1 = i / NS, (i + 1) / NS
        # keep each print left→right when its face is turned to the camera
        pa, pb = pts[a_], pts[b_]
        first, second = (u0, u1) if pa.x < pb.x or kk != 0 else (u1, u0)
        if kk != 0:
            first, second = (u1, u0)
        for l, (uu, vv) in zip(f.loops, ((first, 0), (second, 0), (second, 1), (first, 1))):
            l[uv].uv = (uu, vv)
    bm.faces.new(near[::-1]); bm.faces.new(far)
    me = bpy.data.meshes.new('slat'); bm.to_mesh(me); bm.free()
    o = C.link(bpy.data.objects.new('slat', me))
    for m in TVM:
        o.data.materials.append(m)
    md = o.modifiers.new('b', 'BEVEL'); md.width = 0.0025; md.segments = 2
    o.location = (x0w + sw * (i + 0.5), (y0w + y1w) / 2, tz + 0.03 + sr + 0.002)
    SLATS.append(o)


def ease_back(t):
    t = max(0.0, min(1.0, t)); c1 = 1.70158; c3 = c1 + 1
    return 1 + c3 * (t - 1) ** 3 + c1 * (t - 1) ** 2


def trivision(t):
    for i, o in enumerate(SLATS):
        k = (t * 1.7 - i / NS * 0.7) / 0.55
        o.rotation_euler = (0, math.radians(120) * ease_back(k), 0)


trivision(TV)
C.rbox('tvglass', TX0 - 4, TY0 - 4, TX1 + 4, TY1 + 4, tz + 0.27, tz + 0.285, 18, 1, C.GLASS)
# a tiny engraved plate under the window
C.text('TRIVISION · 24', TX0 + 2, TY1 + 0, 8, MI, C.GREY, BZ + 0.001) if False else None

# lyrics display: several lines like a teleprompter — the sung one big and lit, the rest fading away
LX0, LY0, LX1, LY1 = 610, 190, 1370, 634
lz = bwell('ly', LX0, LY0, LX1, LY1, 22, 0.08, C.mat('lywell', hexc(T['well']), 0.85, bump=(900, 0.04)))
LZ = lz + 0.001
C.cyl('lyd', LX0 + 24, LY0 + 26, 3.5, lz, lz + 0.004, C.LEDG)
C.text('KICK BACK', LX0 + 38, LY0 + 32, 16, MI, C.WHITE, LZ)
C.text('Kenshi Yonezu', LX0 + 146, LY0 + 32, 16, IN, C.GREY, LZ)
xx = LX1 - 18
for lab, on in (('日本', DOWN), ('ROM', DOWN or not APP), ('UZ', DOWN)):
    w_ = 44 if lab != 'ROM' else 52
    k, zt = C.key('lang' + lab, xx - w_, LY0 + 14, xx, LY0 + 40, 8, on, m=C.KEYS, z=lz)
    C.text(lab, xx - w_ / 2, LY0 + 32, 11, DOT if lab == '日本' else MI, C.WHITE if on else C.GREY, zt + 0.001, align='CENTER')
    xx -= w_ + 6
C.rbox('lysep', LX0 + 20, LY0 + 52, LX1 - 20, LY0 + 53, lz, lz + 0.002, 0, 0, C.mat('sep', hexc('#2a2c31'), 0.5))


def lyr_mat(name, col, e):
    return C.mat(name, hexc(col), 0.5, emit=hexc(col), estr=e)


L_PAST2, L_PAST1 = lyr_mat('lp2', '#3f4730', 0.5), lyr_mat('lp1', '#5f6b44', 0.9)
L_NOW_REST = lyr_mat('lnr', '#8a9a5c', 1.4)
L_NEXT1, L_NEXT2, L_NEXT3 = lyr_mat('ln1', '#6f7c4c', 1.0), lyr_mat('ln2', '#4f5838', 0.7), lyr_mat('ln3', '#373e28', 0.45)
LXT = LX0 + 30
C.text("oldingi qator shu yerda edi", LXT, LY0 + 100, 24, VT, L_PAST2, LZ)
C.text("hozirgisidan oldingi qator", LXT, LY0 + 140, 28, VT, L_PAST1, LZ)
C.text("Qo'shiq matni shu yerda", LXT, LY0 + 214, 54, VT, C.LYR, LZ)
C.text("qator-qator", LXT + 440, LY0 + 214, 54, VT, L_NOW_REST, LZ)
C.text("O'zbekcha tarjima shu yerda turadi", LXT + 2, LY0 + 246, 17, IN, lyr_mat('ltr', '#9fb070', 0.9), LZ)
C.text("keyingi qator xira bo'lib kutadi", LXT, LY0 + 312, 30, VT, L_NEXT1, LZ)
C.text("undan keyingisi yana xiraroq", LXT, LY0 + 352, 26, VT, L_NEXT2, LZ)
C.text("va shu tarzda pastga qarab so'nadi", LXT, LY0 + 388, 22, VT, L_NEXT3, LZ)
# a thin progress rail of line dots on the right edge
for j in range(14):
    yy = LY0 + 90 + j * 22
    on = j == 5
    C.rbox('lydot', LX1 - 26, yy, LX1 - 20, yy + (14 if on else 6), lz, lz + 0.004, 3, 0, C.LYR if on else lyr_mat('ldot%d' % j, '#3a4128', 0.4))

# ───────────────────────────── deck: the big LCD under glass
LZ0 = -0.09
C.rbox('lcdframe', 128, 672, 1392, 834, LZ0, LZ0 + 0.02, 22, 1.5, C.LCDFRAME)
C.rbox('lcd', 133, 677, 1387, 829, LZ0, LZ0 + 0.024, 18, 0, C.LCD)
LT = LZ0 + 0.0245
C.icon('NOTE', 164, 700, 1.6, C.LCDINK, LT)
C.text('KICK BACK · Kenshi Yonezu', 182, 707, 20, DOT, C.LCDINK, LT)
C.text('8', 1218, 716, 46, VT, C.LCDGHOST, LT)
C.text('1:42', 1238, 716, 46, VT, C.LCDINK, LT)
C.text('/ 3:13', 1368, 716, 22, VT, C.LCDGHOST, LT, align='RIGHT')
random.seed(7)
n = 180
for i in range(n):
    x0 = 160 + i * (1200 / n)
    hg = min(8 + abs(math.sin(i * .43) * 30 + math.sin(i * .13) * 16 + random.random() * 8), 54)
    C.rbox('wv', x0, 754 - hg / 2, x0 + 3.8, 754 + hg / 2, LT - 0.001, LT + 0.0015, 0, 0, C.LCDINK if i < 96 else C.LCDGHOST)
C.rbox('head', 160 + 96 * (1200 / n), 724, 163 + 96 * (1200 / n), 786, LT - 0.001, LT + 0.002, 0, 0, C.LCDINK)
C.rbox('lline', 160, 794, 1360, 795, LT - 0.001, LT + 0.0012, 0, 0, C.LCDGHOST)
C.text('UZ o   ROM *   JP o        J-POP · 172 BPM · ENERGIYA 92%', 162, 818, 22, VT, C.LCDINK, LT)
C.text('ON AIR *', 1360, 818, 22, VT, C.LCDINK, LT, align='RIGHT')
C.rbox('glass', 131, 675, 1389, 831, LZ0 + 0.05, LZ0 + 0.065, 20, 1, C.GLASS)

# ───────────────────────────── deck controls
for nm, cx, ic in (('play', 186, 'PLAY'), ('heart', 830, 'HEART')):
    C.cyl(nm + 'w', cx, 912, 54, -0.06, 0.0005, C.TRAY)
    pd = APP and DOWN
    top = 0.06 if pd else 0.12
    C.cyl(nm, cx, 912, 47, -0.06, top, C.KEY, bev=6)
    if pd:
        ic = 'PAUSE' if ic == 'PLAY' else 'HEART_FULL'
    C.icon(ic, cx + (3 if ic == 'PLAY' else 0), 912, 3.1, C.RED if ic == 'HEART_FULL' else INK, top)

# tuner drum
c = C.rbox('cut_drum', 256, 864, 760, 960, -2.6, 0.3, 22, 0, C.CHALK); C.cut(chassis, c)
C.rbox('drumfloor', 254, 862, 762, 962, -2.7, -2.6, 22, 0, C.WELL)
R = 2.42
DRUM = C.mat('drumm', hexc('#eef0f3') if not DARK else hexc('#3a3d44'), 0.32, coat=0.35, emit=hexc('#eef0f3') if not DARK else hexc('#3a3d44'), estr=0.4 if not DARK else 0.25)
bm = bmesh.new(); bmesh.ops.create_cone(bm, cap_ends=True, segments=256, radius1=R, radius2=R, depth=0.9)
me = bpy.data.meshes.new('drum'); bm.to_mesh(me); bm.free()
for p_ in me.polygons:
    p_.use_smooth = True
dr = C.link(bpy.data.objects.new('drum', me))
dx, dy = P(508, 912)
dr.location = (dx, dy, -R + 0.02); dr.rotation_euler = (math.radians(90), 0, 0); dr.data.materials.append(DRUM)
TICK = C.INK if not DARK else C.mat('tickd', hexc('#c9cdd4'), 0.5)
for i in range(-42, 43):
    th = math.radians(i * 2.0)
    if abs(th) > math.radians(84):
        continue
    long = i % 5 == 0
    bm = bmesh.new(); bmesh.ops.create_cube(bm, size=1); me = bpy.data.meshes.new('tk'); bm.to_mesh(me); bm.free()
    t = C.link(bpy.data.objects.new('tick', me)); t.data.materials.append(TICK)
    t.scale = (0.024 if long else 0.012, 0.5 if long else 0.32, 0.006)
    t.location = (dx + R * math.sin(th), dy, -R + 0.02 + R * math.cos(th) + 0.002)
    t.rotation_euler = (0, th, 0)
C.rbox('needle', 502, 872, 514, 952, 0.05, 0.08, 6, 1.5, C.RED)
C.rbox('drumglass', 258, 866, 758, 958, 0.03, 0.045, 20, 1, C.GLASS)

for nm, x0, x1, ic, lab in (('tavsif', 128, 444, 'INFO', 'TAVSIF'), ('saqlash', 454, 764, 'SAVE', 'SAQLASH')):
    k, zt = C.key(nm, x0, 976, x1, 1058, 24, DOWN if APP else nm == 'tavsif', z=-0.06)
    C.icon(ic, (x0 + x1) / 2 - 58, 1017, 2.3, INK, zt)
    C.text(lab, (x0 + x1) / 2 - 34, 1025, 17, MI, INK, zt + 0.001, spacing=1.1)
    if nm == 'tavsif':
        C.dome('tvled', x1 - 26, 994, 4.2, zt, C.LED_ON if (DOWN or not APP) else C.LED_DIM)

# speaker grille
C.rbox('grillep', 886, 864, 1010, 1056, -0.05, -0.02, 18, 1.5, C.ALUD)
for j in range(24):
    for i in range(14):
        xx = 896 + i * 8 + (4 if j % 2 else 0)
        yy = 874 + j * 7.9
        if xx > 1002 or yy > 1048:
            continue
        C.cyl('h', xx, yy, 2.2, -0.0205, -0.0195, C.WELL, segs=12)

# prev / next
for nm, y0, ic in (('prev', 862, 'PREV'), ('next', 962, 'NEXT')):
    k, zt = C.key(nm, 1036, y0, 1096, y0 + 94, 18, APP and DOWN, z=-0.06)
    C.icon(ic, 1066, y0 + 47, 2.6, INK, zt)

# volume knob: knurled aluminium, a ring of real dome LEDs, a printed 0–10 scale
kx, ky = 1250, 958
for i in range(21):
    a = math.radians(225 - i * 270 / 20)
    on = DOWN if APP else i <= 14
    m = (C.LED_HOT if i >= 18 else C.LED_ON) if on else (C.LED_DIMR if i >= 18 else C.LED_DIM)
    lx, ly_ = kx + math.cos(a) * 86, ky - math.sin(a) * 86
    C.cyl('lb%d' % i, lx, ly_, 6.6, kwz - 0.01, kwz + 0.014, C.BEZEL, segs=32)
    C.dome('ld%d' % i, lx, ly_, 4.8, kwz + 0.014, m, h=0.7)
for v in range(0, 11, 2):
    a = math.radians(225 - v * 27)
    C.text(str(v), kx + math.cos(a) * 114, ky - math.sin(a) * 114 + 5, 11.5, MI, INKS, 0.001, align='CENTER')
C.cyl('knob', kx, ky, 64, kwz, 0.36, C.ALU, bev=2, segs=160)
for i in range(140):
    a = i / 140 * math.tau
    bm = bmesh.new(); bmesh.ops.create_cube(bm, size=1); me = bpy.data.meshes.new('rb'); bm.to_mesh(me); bm.free()
    rb = C.link(bpy.data.objects.new('rib', me)); rb.data.materials.append(C.ALUD)
    px_, py_ = P(kx, ky)
    rb.location = (px_ + math.cos(a) * 0.64, py_ + math.sin(a) * 0.64, (kwz + 0.33) / 2)
    rb.scale = (0.024, 0.018, 0.33 - kwz)
    rb.rotation_euler = (0, 0, a)
C.cyl('kcap', kx, ky, 52, 0.30, 0.38, C.ALU, bev=2.5, segs=160)
a = math.radians(225 - 14 * 13.5)
C.cyl('kdot', kx + math.cos(a) * 38, ky - math.sin(a) * 38, 5.5, 0.37, 0.386, C.RED, bev=1.2)
C.text('OVOZ', 1356, 1056, 10.5, MI, INKS, 0.001, align='RIGHT')

# ───────────────────────────── right: the TAVSIF thermal printer
# Long texts: the paper runs like a scroll between the head (top) and a take-up roller with a little crank
# (bottom); the mouse wheel winds it, the crank turns, the head's LCD counts the lines.
PZ = -0.07
SCROLL = arg('--scroll', 0.0, float)
hb = C.rbox('phead', 1418, 24, 1896, 118, PZ - 0.05, 0.16, 24, 3, C.KEY if not DARK else C.mat('headd', hexc('#26292e'), 0.45, coat=0.2))
C.text('TAVSIF', 1442, 62, 19, DELA, INKD, 0.161, spacing=1.12)
C.text('THERMAL PRINTER · KR-P1', 1442, 82, 10, MI, INKS, 0.161, spacing=1.1)
C.cyl('pledb', 1866, 50, 7.5, 0.15, 0.17, C.BEZEL, segs=32)
C.dome('pled', 1866, 50, 5.4, 0.17, (C.LED_DIM if APP and not DOWN else C.mat('ledg2', hexc('#7dffbf'), 0.15, coat=1, emit=hexc('#3ee08f'), estr=3)))
WIN_KEYS = [('min', 1592), ('max', 1628), ('close', 1664)]
for nm, cx in WIN_KEYS:
    pressed = APP and DOWN
    C.cyl('wkw_' + nm, cx, 58, 16, 0.12, 0.161, C.BEZEL if DARK else C.mat('wkwl', hexc('#c9cdd5'), 0.5), segs=48)
    top = 0.17 if pressed else 0.19
    C.cyl('wk_' + nm, cx, 58, 13, 0.12, top, C.KEY if not DARK else C.mat('wkd', hexc('#3a3d44'), 0.4, coat=0.3), bev=3, segs=48)
    ink = C.RED if nm == 'close' else (C.INK if not DARK else C.mat('wki', hexc('#c9cdd4'), 0.5))
    z = top + 0.001
    if nm == 'min':
        C.rbox('wi', cx - 6, 57, cx + 6, 59.6, z, z + 0.004, 1, 0, ink)
    elif nm == 'max':
        for a_, b_, c_, d_ in ((cx - 6, 52, cx + 6, 54.2), (cx - 6, 61.8, cx + 6, 64), (cx - 6, 52, cx - 3.8, 64), (cx + 3.8, 52, cx + 6, 64)):
            C.rbox('wi', a_, b_, c_, d_, z, z + 0.004, 0.5, 0, ink)
    else:
        for ang in (45, -45):
            o = C.rbox('wi', cx - 7, 57, cx + 7, 59.4, z, z + 0.004, 1, 0, ink)
            px_, py_ = P(cx, 58.2)
            o.data.transform(__import__('mathutils').Matrix.Translation((-px_, -py_, 0)))
            o.location = (px_, py_, 0)
            o.rotation_euler = (0, 0, math.radians(ang))
    if pressed and nm == 'close':
        C.dome('wkled', cx, 58, 3, top + 0.002, C.LED_HOT, h=0.3) if False else None
C.rbox('qframe', 1700, 40, 1846, 76, 0.155, 0.17, 8, 1, C.LCDFRAME)
C.rbox('qlcd', 1703, 43, 1843, 73, 0.155, 0.172, 6, 0, C.LCD)
QTXT = C.text('QATOR 01/31', 1773, 66, 18, VT, C.LCDINK, 0.1725, align='CENTER')
C.rbox('slot', 1458, 98, 1856, 106, 0.14, 0.165, 3, 0, C.WELL)
VENTM = C.mat('ventm', hexc('#08090a'), 0.95, spec=0.05)
for vx in (1426, 1446, 1852, 1872):
    for k in range(9):
        vy = 150 + k * 38
        C.rbox('vent', vx, vy, vx + 10, vy + 28, PZ + 0.0004, PZ + 0.0016, 3, 0, VENTM)
        C.rbox('ventlip', vx + 1, vy + 28, vx + 9, vy + 29.2, PZ + 0.0004, PZ + 0.0018, 0, 0, C.mat('ventl', hexc('#ffffff') if not DARK else hexc('#44474e'), 0.5))
C.rbox('lip', 1452, 106, 1862, 110, 0.15, 0.17, 2, 0.5, C.CHROME)
RY = 536
for nm, rad, ln, m in (('roller', 15, 3.96, C.CHROME), ('rub', 16, 3.64, C.mat('rubr', hexc('#1a1b1e'), 0.7))):
    o = C.cyl(nm, 0, 0, rad, -0.5, 0.5, m, segs=64)
    o.scale = (1, 1, ln); o.location = (*P(1657, RY), PZ + 0.04); o.rotation_euler = (0, math.radians(90), 0)
for xx in (1452, 1862):
    C.cyl('rcap', xx, RY, 18, PZ, PZ + 0.11, C.ALU, bev=1.5, segs=48)
crank = bpy.data.objects.new('crank', None); C.link(crank)
cx_, cy_ = P(1878, RY)
crank.location = (cx_, cy_, 0)
cb = C.rbox('carm', 1872, RY - 4, 1906, RY + 4, PZ + 0.11, PZ + 0.13, 4, 1, C.ALU)
cb.data.transform(Matrix.Translation((-cx_, -cy_, 0))); cb.parent = crank
kb = C.cyl('cknob', 1900, RY, 6, PZ + 0.11, PZ + 0.2, C.mat('knobred', hexc('#e0313f'), 0.3, coat=0.6), bev=1.5)
kb.location.x -= cx_; kb.location.y -= cy_; kb.parent = crank
PZT = PZ + 0.02


def zig(x0, x1, ytop, ybot, teeth=26, depth=8, end=True):
    bm = bmesh.new()
    pts = [(x0, ytop), (x1, ytop)]
    if end:
        step = (x1 - x0) / teeth
        for i in range(teeth, -1, -1):
            pts.append((x0 + i * step, ybot - (depth if i % 2 else 0)))
    else:
        pts += [(x1, ybot), (x0, ybot)]
    vs = [bm.verts.new((*P(px, py), 0)) for px, py in pts]
    bm.faces.new(vs)
    bmesh.ops.triangulate(bm, faces=bm.faces[:])
    me = bpy.data.meshes.new('pp'); bm.to_mesh(me); bm.free()
    return me


po = C.link(bpy.data.objects.new('paperm', zig(1478, 1836, 100, RY, end=False))); po.data.materials.append(C.PAPER); po.location.z = PZT
sol = po.modifiers.new('s', 'SOLIDIFY'); sol.thickness = 0.004; sol.offset = 0
paper = bpy.data.objects.new('paper', None); C.link(paper)
INKP = C.THERMAL
INKP2 = C.mat('thermal2', hexc('#77777a'), 0.8)
PRINTED = []


def pt(body, x, y, size, fnt=DOT, m=None, align='LEFT', spacing=1.0):
    o = C.text(body, x, y, size, fnt, m or INKP, PZT + 0.007, align=align, spacing=spacing)
    o.parent = paper
    PRINTED.append((o, y, y - size * 0.8))
    return o


def prect(x0, y0, x1, y1, m=None):
    o = C.rbox('pr', x0, y0, x1, y1, PZT + 0.0055, PZT + 0.007, 0, 0, m or INKP)
    o.parent = paper
    PRINTED.append((o, y1, y0))
    return o


pt('* KAWAII RADIO *', 1657, 132, 15, MI, align='CENTER', spacing=1.1)
pt("KR-01 · QO'SHIQ TAVSIFI", 1657, 150, 12, DOT, INKP2, align='CENTER')
for xx in range(1494, 1822, 12):
    prect(xx, 162, xx + 6, 163.5, INKP2)
pt('KICK BACK', 1494, 198, 26, DELA)
pt('Kenshi Yonezu', 1495, 222, 16, DOT, INKP2)
pt('J-POP · 2022 · 3:13 · 172 BPM', 1494, 248, 14, DOT)
pt('ENERGIYA', 1494, 274, 12, DOT, INKP2)
for k in range(12):
    prect(1572 + k * 17, 262, 1585 + k * 17, 276, INKP if k < 11 else INKP2)
pt('92%', 1822, 274, 12, DOT, align='RIGHT')
for xx in range(1494, 1822, 12):
    prect(xx, 288, xx + 6, 289.5, INKP2)
DESC = ["Kenshi Yonezu «Chainsaw Man»", "animesi uchun yozgan opening.", "2022-yil oktyabrida chiqqan,",
        "AQShda «Gold» olgan birinchi", "yaponcha qo'shiq bo'ldi. Arra", "ovozi, keskin ritm, baqiriqli",
        "naqarot — animening o'zi.", "", "YARATILISH TARIXI", "Qo'shiq anime ruhiga mos:",
        "Denji ichidagi tartibsizlik", "va oddiy hayotga ochlik", "ritmda to'qnashadi.", "", "QIZIQ FAKT",
        "Klipda Yonezu o'zi raqsga", "tushadi, sahnalar esa", "anime kadrlariga ishora."]
DY = 314
for i, l in enumerate(DESC):
    if l and l.isupper():
        pt(l, 1494, DY + i * 21, 12, MI, INKP2, spacing=1.1)
    elif l:
        pt(l, 1494, DY + i * 21, 14, DOT)
yend = DY + len(DESC) * 21 + 6
for xx in range(1494, 1822, 12):
    prect(xx, yend, xx + 6, yend + 1.5, INKP2)
pt('#0412   21.10.2026   23:52', 1494, yend + 22, 12, DOT, INKP2)
random.seed(3)
xx = 1494
while xx < 1820:
    w_ = random.choice((1.6, 1.6, 3.2, 4.8))
    prect(xx, yend + 32, xx + w_, yend + 56)
    xx += w_ + random.choice((1.6, 3.2))
PAPER_END = yend + 70
TOTAL_LINES = 31


def wind(scroll, edge=None):
    """Scroll the receipt by `scroll` px; `edge` = how far the printed part reaches (print animation)."""
    paper.location = (0, scroll / 100, 0)
    end = PAPER_END - scroll
    bottom = min(RY, end) if edge is None else min(edge, RY, end)
    po.data = zig(1478, 1836, 100, max(bottom, 112), end=end <= RY and (edge is None or edge >= end))
    po.data.materials.append(C.PAPER)
    for o, yb, yt in PRINTED:
        o.hide_render = (yt - scroll) < 110 or (yb - scroll) > min(RY - 10, bottom - 4)
    crank.rotation_euler = (0, 0, -scroll / 40)
    line = int(max(0, scroll) / 21) + 1
    QTXT.data.body = 'QATOR %02d/%02d' % (min(line, TOTAL_LINES), TOTAL_LINES)


wind(SCROLL)

# ───────────────────────────── right: chat
_before = set(sc.objects)
CZ = -0.07
C.text('CHAT', 1432, 630, 26, DELA, INKD, CZ + 0.001, spacing=1.08)
C.text('214 onlayn · 31 radioda', 1568, 626, 15, IN, INKS, CZ + 0.001)
C.cyl('lvb', 1838, 618, 7, CZ - 0.01, CZ + 0.012, C.BEZEL, segs=32)
C.dome('lv', 1838, 618, 5, CZ + 0.012, C.mat('ledg3', hexc('#7dffbf'), 0.15, coat=1, emit=hexc('#3ee08f'), estr=3))
C.text('LIVE', 1852, 624, 11, MI, C.mat('greenink', hexc('#1f8a55') if not DARK else hexc('#3ee08f'), 0.5), CZ + 0.001)
k, zt = C.key('pin', 1426, 642, 1888, 690, 16, z=CZ)
C.rbox('pinbar', 1440, 650, 1443.5, 682, zt - 0.002, zt + 0.003, 1, 0, C.GOLD)
C.text("QADALGAN · SYNTX", 1454, 662, 9.5, MI, INKS, zt + 0.001, spacing=1.1)
C.text("!! Ilovada yangimisiz?", 1454, 683, 16, IN, INKD, zt + 0.001)
BUBS = [
    ('fujii-kaze-matsuri-mv84iqkb', 'sakura', None, ["salom hammaga :)", "bugun nima eshitamiz?"], 702, 780),
    ('eve-underdog-mv84in6q', 'syntx', ('PATROL', '#3b6fe0'), ["Tokyo Ghetto · Eve ulashdi"], 786, 846),
    ('ado-odo-mv84hade', 'Last veRum', ('GOLD II', '#e8b931'), ["wassup qoravoyla <3"], 860, 920),
    ('kenshi-yonezu-iris-out-mv84ixgy', 'z.ortiqov', ('EMERALD', '#34c08f'), ["KICK BACK efirda, ovozni balandroq!"], 934, 994),
]
for img, who, badge, msg, y0, y1 in BUBS:
    am = C.image_mat('av_' + who, os.path.join(lib.TEX, img + '.jpg'), coat=0.3)
    C.cyl('avr', 1446, y1 - 18, 19, CZ, CZ + 0.08, C.RIM, bev=2)
    C.plane('av', 1429, y1 - 35, 1463, y1 - 1, CZ + 0.0815, am, r=17)
    wdt = 400 if len(msg[0]) > 26 else 300
    k, zt = C.key('bub', 1474, y0, 1474 + wdt, y1, 18, z=CZ)
    C.text(who, 1490, y0 + 23, 15, IN6, INK, zt + 0.001)
    if badge:
        bw = 14.5 * len(badge[0]) + 20
        bx = 1490 + 9.2 * len(who) + 12
        C.rbox('bdg', bx, y0 + 8, bx + bw, y0 + 28, zt, zt + 0.004, 5, 0, C.mat('bd' + who, hexc(badge[1]), 0.4))
        C.text(badge[0], bx + bw / 2, y0 + 23, 11, MI, C.WHITE if badge[0] != 'GOLD II' else C.DARKINK, zt + 0.0045, align='CENTER')
    for i, l in enumerate(msg):
        C.text(l, 1490, y0 + 49 + i * 21, 17, IN, INKD, zt + 0.001)
k, zt = C.key('cmpk', 1426, 1008, 1478, 1052, 14, z=CZ)
C.icon('RADIO', 1452, 1030, 1.7, INK, zt)
C.rbox('inframe', 1486, 1006, 1832, 1054, CZ, CZ + 0.02, 14, 1.5, C.LCDFRAME)
C.rbox('inlcd', 1490, 1010, 1828, 1050, CZ, CZ + 0.024, 11, 0, C.LCD)
C.text('XABAR YOZING', 1506, 1037, 19, VT, C.LCDGHOST, CZ + 0.0245, spacing=1.15)
C.rbox('caret', 1624, 1018, 1626.5, 1042, CZ + 0.0245, CZ + 0.026, 0, 0, C.LCDINK)
k, zt = C.key('smile', 1840, 1008, 1888, 1052, 14, z=CZ)
C.text(':)', 1864, 1038, 18, DOT, INK, zt + 0.001, align='CENTER')

CHATROOT = bpy.data.objects.new('chatroot', None); C.link(CHATROOT)
CHAT_LEDS = []
for o in set(sc.objects) - _before - {CHATROOT}:
    if o.parent is None:
        o.parent = CHATROOT
    if o.name.startswith('lv') and o.type == 'MESH':
        CHAT_LEDS.append(o)

C.bloom(0.35, 1.3, 0.5)

if APP:
    DYN_TEXT = {'214 TINGLOVCHI', 'KICK BACK', 'Kenshi Yonezu', '8', '1:42', '/ 3:13', 'ON AIR *',
                'UZ o   ROM *   JP o        J-POP · 172 BPM · ENERGIYA 92%', 'KICK BACK · Kenshi Yonezu'}
    for o in list(sc.objects):
        n = o.name.split('.')[0]
        hide = False
        if o.type == 'FONT' and (o.data.body in DYN_TEXT or o.data.materials and o.data.materials[0].name.split('.')[0] in
                                 ('lyr', 'lp2', 'lp1', 'lnr', 'ln1', 'ln2', 'ln3', 'ltr')):
            hide = True
        if n in ('slat', 'tvglass', 'lyd', 'lydot', 'wv', 'head', 'lline', 'ic_NOTE', 'tick', 'kdot', 'paperm'):
            hide = True
        if o.parent is CHATROOT or o is QTXT:
            hide = True
        if hide:
            o.hide_render = True
    for o, *_ in PRINTED:
        o.hide_render = True
    for o in CHATROOT.children_recursive:
        o.hide_render = True

reg = arg('--region')
if reg:
    x0, y0, x1, y1 = [float(v) for v in reg.split(',')]
    sc.render.use_border = True
    sc.render.use_crop_to_border = True
    sc.render.border_min_x = x0 / W; sc.render.border_max_x = x1 / W
    sc.render.border_min_y = 1 - y1 / H; sc.render.border_max_y = 1 - y0 / H
sc.render.filepath = os.path.abspath(arg('--out', os.path.join(lib.HERE, '..', 'g-%s.png' % THEME)))
ANIMP = arg('--anim-print')
ANIMS = arg('--anim-scroll')
ANIMC = arg('--anim-chat')
ANIMT = arg('--anim-tv')


def ease_out(t):
    t = max(0.0, min(1.0, t))
    return 1 - (1 - t) ** 3


if ANIMT:
    out = os.path.abspath(ANIMT); os.makedirs(out, exist_ok=True)
    for f in range(60):
        trivision(min(1.0, f / 44))
        sc.render.filepath = os.path.join(out, '%04d.png' % f)
        bpy.ops.render.render(write_still=True)
    print('ANIM DONE')
    raise SystemExit
if ANIMP or ANIMS:
    out = os.path.abspath(ANIMP or ANIMS); os.makedirs(out, exist_ok=True)
    N = 72 if ANIMP else 100
    for f in range(N):
        if ANIMP:
            t = min(1.0, f / 54); steps = 36
            q = math.floor(t * steps) / steps + (t * steps % 1) ** 3 / steps
            wind(0, 104 + (RY - 104) * min(q, 1.0))
        else:
            # two wheel flicks down with inertia, a small one back up
            wind(150 * ease_out(f / 26) + 130 * ease_out((f - 32) / 26) - 50 * ease_out((f - 70) / 20))
        sc.render.filepath = os.path.join(out, '%04d.png' % f)
        bpy.ops.render.render(write_still=True)
    print('ANIM DONE')
    raise SystemExit
if ANIMC:
    out = os.path.abspath(ANIMC); os.makedirs(out, exist_ok=True)
    LCDB = C.LCD.node_tree.nodes['Principled BSDF']
    for f in range(64):
        # out of the side on rails, a small overshoot and the latch, then the LEDs and the LCD power up
        t = f / 22
        ov = ease_out(t) + (0.025 * math.sin(min(1.0, max(0.0, (f - 22) / 8)) * math.pi) if f >= 22 else 0)
        CHATROOT.location = (5.2 * (1 - ov), 0, 0)
        power = max(0.0, min(1.0, (f - 30) / 12))
        flicker = power > 0 and power < 1 and f % 3 == 0
        for o in CHAT_LEDS:
            o.hide_render = power <= 0.0 or flicker
        LCDB.inputs['Emission Strength'].default_value = T['lcdEmit'] * (0.0 if power <= 0 or flicker else power)
        sc.render.filepath = os.path.join(out, '%04d.png' % f)
        bpy.ops.render.render(write_still=True)
    print('ANIM DONE')
    raise SystemExit
if '--save' in argv:
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(lib.HERE, 'kr01-%s.blend' % THEME))
bpy.ops.render.render(write_still=True)
print('WROTE', sc.render.filepath)
