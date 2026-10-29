#!/usr/bin/env python3
"""Generate test inputs: MusicXML files and a raw MIDI file."""
import struct, os

OUT = os.path.dirname(os.path.abspath(__file__)) + "/../web/samples"
os.makedirs(OUT, exist_ok=True)

def xml_header(title, composer):
    return f'''<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 3.1 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd">
<score-partwise version="3.1">
 <work><work-title>{title}</work-title></work>
 <identification><creator type="composer">{composer}</creator></identification>
'''

def part_attrs(divisions=4):
    return f'''  <attributes>
   <divisions>{divisions}</divisions>
   <time><beats>4</beats><beat-type>4</beat-type></time>
   <clef><sign>G</sign><line>2</line></clef>
  </attributes>
'''

STEP = {0:('C',0),1:('C',1),2:('D',0),3:('D',1),4:('E',0),5:('F',0),6:('F',1),7:('G',0),8:('G',1),9:('A',0),10:('A',1),11:('B',0)}

def note(midi, dur, tie=None):
    if midi is None:
        return f'   <rest><duration>{dur}</duration></rest>\n'
    step, alter = STEP[midi % 12]
    oct = midi // 12 - 1
    t = ''
    if tie == 'start': t = '   <tie type="start"/>\n'
    if tie == 'stop':  t = '   <tie type="stop"/>\n'
    n = ''
    if tie:
        n = f'   <notations><tie type="{tie}"/></notations>\n'
    a = f'<alter>{alter}</alter>' if alter else ''
    return (f'   <note>\n    <pitch><step>{step}</step>{a}<octave>{oct}</octave></pitch>\n'
            f'    <duration>{dur}</duration>\n{t}{n}   </note>\n')

def chord(pitches, dur):
    out = ''
    for i, p in enumerate(pitches):
        step, alter = STEP[p % 12]
        oct = p // 12 - 1
        a = f'<alter>{alter}</alter>' if alter else ''
        c = '   <chord/>\n' if i > 0 else ''
        out += (f'   <note>\n{c}    <pitch><step>{step}</step>{a}<octave>{oct}</octave></pitch>\n'
                f'    <duration>{dur}</duration>\n   </note>\n')
    return out

def measure(content):
    return f' <measure number="{measure.n}">\n{content} </measure>\n' if False else content

def m(num, content):
    return f' <measure number="{num}">\n{content} </measure>\n'

# ---------- Twinkle, two parts ----------
mel = [60,60,67,67,69,69,67,  65,65,64,64,62,62,60]
harm_roots = [60,65,67,65,60,65,60,  65,60,62,55,60,65,60]
xml = xml_header("Twinkle Twinkle", "Traditional")
xml += ' <part-list>\n  <score-part id="P1"><part-name>Rights</part-name></score-part>\n  <score-part id="P2"><part-name>Lefts</part-name></score-part>\n </part-list>\n'
xml += ' <part id="P1">\n'
mi = 0
for b in range(7):
    c = part_attrs() if b == 0 else ''
    for k in range(2):
        c += note(mel[mi], 2); mi += 1
    xml += m(b+1, c)
xml += ' </part>\n <part id="P2">\n'
for b in range(7):
    c = part_attrs() if b == 0 else ''
    r = harm_roots[b*2]
    c += chord([r-12, r, r+4, r+7], 4)
    r2 = harm_roots[b*2+1]
    c += chord([r2-12, r2, r2+4, r2+7], 4)
    xml += m(b+1, c)
xml += ' </part>\n</score-partwise>\n'
open(f"{OUT}/twinkle.musicxml","w").write(xml)

# ---------- Chromatic single part (tests accidentals + middle-C split) ----------
xml = xml_header("Chromatic Exercise", "Clean Room")
xml += ' <part-list>\n  <score-part id="P1"><part-name>Chromatic</part-name></score-part>\n </part-list>\n <part id="P1">\n'
mi = 0
seq = list(range(48, 73))
for b in range(6):
    c = part_attrs() if b == 0 else ''
    for k in range(4):
        c += note(seq[mi], 1); mi += 1
    xml += m(b+1, c)
xml += ' </part>\n</score-partwise>\n'
open(f"{OUT}/chromatic.musicxml","w").write(xml)

# ---------- 3/4 with ties across barlines ----------
xml = xml_header("Waltz Test", "Test Composer")
xml += ' <part-list>\n  <score-part id="P1"><part-name>Waltz</part-name></score-part>\n </part-list>\n <part id="P1">\n'
xml += m(1, part_attrs(2).replace('<beats>4</beats><beat-type>4</beat-type>','<beats>3</beats><beat-type>4</beat-type>')
         + note(64, 6, 'start') + note(67, 6, 'start') + note(71, 6))
xml += m(2, note(64, 6, 'stop') + note(69, 6, 'start') + note(67, 6))
xml += m(3, note(69, 3, 'stop') + note(65, 3) + note(64, 6))
xml += ' </part>\n'
# left part: notes held across bar lines -> continuation dots
xml += ' <part id="P2">\n'
xml += m(1, part_attrs(2).replace('<beats>4</beats><beat-type>4</beat-type>','<beats>3</beats><beat-type>4</beat-type>') + note(48, 12))
xml += m(2, '')
xml += m(3, note(43, 12))
xml += m(4, '')
xml += ' </part>\n</score-partwise>\n'
open(f"{OUT}/waltz.musicxml","w").write(xml)

# ---------- raw MIDI: 2 tracks ----------
def vlq(n):
    out = bytes([n & 0x7f]); n >>= 7
    while n:
        out = bytes([(n & 0x7f) | 0x80]) + out
        n >>= 7
    return out

def track(events):
    b = b''
    last = 0
    for dt, ev in events:
        b += vlq(dt) + ev
        last = dt
    b += vlq(480) + b'\xff\x2f\x00'
    return b'MTrk' + struct.pack('>I', len(b)) + b

# track 0: name + tempo + timesig
t0 = track([(0, b'\xff\x03\x09MIDI Test' ), (0, b'\xff\x51\x03' + (500000).to_bytes(3,'big')), (0, b'\xff\x58\x04\x04\x02\x1c\x08')])
# track 1: C major scale quarter notes C4..C5, track 2: whole-note bass
def notes_msg(ch, pairs):
    evs = []
    t = 0
    for (start, dur, pitch, vel) in pairs:
        evs.append((start - t, bytes([0x90 | ch, pitch, vel]))); t = start
        evs.append((start + dur - t, bytes([0x80 | ch, pitch, 0x64]))); t = start + dur
    return evs

t1 = track(notes_msg(0, [(i*480, 440, 60+i, 96) for i in range(8)]))
t2 = track(notes_msg(1, [(0, 1920, 48, 100), (1920, 1920, 55, 100)]))
mid = b'MThd' + struct.pack('>I', 6) + struct.pack('>HHH', 1, 3, 480) + t0 + t1 + t2
open(f"{OUT}/test.mid","wb").write(mid)
print("wrote samples to", OUT)
