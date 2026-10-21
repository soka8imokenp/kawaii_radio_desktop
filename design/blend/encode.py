"""Frames → MP4 with Blender's own FFmpeg.  blender -b -P encode.py -- frames_dir out.mp4 fps [hold_last_frames] [scale]"""
import bpy, os, sys
a = sys.argv[sys.argv.index('--') + 1:]
src, out, fps = os.path.abspath(a[0]), os.path.abspath(a[1]), int(a[2])
hold = int(a[3]) if len(a) > 3 else 0
files = sorted(f for f in os.listdir(src) if f.endswith('.png'))
sc = bpy.context.scene
img = bpy.data.images.load(os.path.join(src, files[0]))
w, h = img.size
w -= w % 2; h -= h % 2
sc.render.resolution_x, sc.render.resolution_y = w, h
sc.render.resolution_percentage = 100
sc.render.fps = fps
se = sc.sequence_editor_create()
st = se.strips.new_image('f', os.path.join(src, files[0]), 1, 1) if hasattr(se, 'strips') else se.sequences.new_image('f', os.path.join(src, files[0]), 1, 1)
for f in files[1:]:
    st.elements.append(f)
st.frame_final_duration = len(files) + hold
sc.frame_start, sc.frame_end = 1, len(files) + hold
sc.render.image_settings.file_format = 'FFMPEG'
sc.render.ffmpeg.format = 'MPEG4'
sc.render.ffmpeg.codec = 'H264'
sc.render.ffmpeg.constant_rate_factor = 'HIGH'
sc.render.filepath = out
sc.view_settings.view_transform = 'Standard'
bpy.ops.render.render(animation=True)
print('ENCODED', out)
