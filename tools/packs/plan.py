"""Plan the SensePad sample packs.

Reads the file listings of four CC0 drum libraries (saved from the GitHub API as
<repo>.json) and writes manifest.json: for every pack sound, which microphone
recordings are mixed into it. fetch.py downloads those files and the pack builder
in src/packtool.rs turns them into the packs.

    python plan.py <listings dir> manifest.json
"""
import json
import os
import re
import sys

LISTINGS, OUT = sys.argv[1], sys.argv[2]


class Repo:
    def __init__(self, name, branch):
        self.name, self.branch = name, branch
        tree = json.load(open(os.path.join(LISTINGS, name + ".json"), encoding="utf-8"))["tree"]
        self.files = {e["path"]: e.get("size", 0) for e in tree if e["type"] == "blob"}

    def pick(self, folder, pattern, optional=False):
        """The file in `folder` whose name matches `pattern`; the highest group(1) number wins."""
        rx = re.compile(pattern)
        best = None
        for path in sorted(self.files):
            head, tail = os.path.split(path)
            if head != folder:
                continue
            m = rx.fullmatch(os.path.splitext(tail)[0])
            if not m:
                continue
            level = int(m.group(1)) if m.groups() and m.group(1) else 0
            if best is None or level > best[0]:
                best = (level, path)
        if best is None and optional:
            return None
        if best is None:
            names = sorted(os.path.basename(p) for p in self.files if os.path.dirname(p) == folder)
            raise SystemExit("no match for %s in %s/%s; have %s" % (pattern, self.name, folder, names[:40]))
        return {"repo": self.name, "branch": self.branch, "path": best[1], "size": self.files[best[1]]}


UNRULY = Repo("karoryfer.unruly-drums", "main")
SWIRLY = Repo("karoryfer.swirly-drums", "main")
FRANKEN = Repo("karoryfer.frankensnare", "main")
VIRTUO = Repo("virtuosity_drums", "master")

LEVEL = r"(?:_vl(\d+))?"


def src(found, gain, pan=0.0):
    found = dict(found)
    found["gain"], found["pan"] = gain, pan
    return found


def unruly(inst, art, mics):
    out = []
    for mic, g, p in mics:
        # Hits played with the snare wires off have no bottom-microphone recording.
        found = UNRULY.pick("Samples/%s/%s/rr1" % (inst, mic), re.escape(art) + LEVEL + "_rr1", optional=mic == "btm")
        if found:
            out.append(src(found, g, p))
    return out


def swirly(inst, art, mics=(("", 1.0),)):
    return [src(SWIRLY.pick("Samples/" + inst, re.escape(art) + LEVEL + "_rr1" + suffix), g) for suffix, g in mics]


def franken(inst, art="sn"):
    mics = (("top", 1.0), ("btm", 0.6), ("oh", 0.6))
    return [src(FRANKEN.pick("Samples/%s/%s" % (inst, mic), re.escape(art) + LEVEL + "_rr1"), g) for mic, g in mics]


def franken_one(folder, pattern):
    return [src(FRANKEN.pick("Samples/" + folder, pattern), 1.0)]


def virtuo(inst, art, mics):
    return [src(VIRTUO.pick("Samples/%s/%s" % (mic, inst), mic + "_" + re.escape(art) + LEVEL + "(?:_rr1)?"), g) for mic, g in mics]


def perc(inst, pattern):
    return [src(VIRTUO.pick("Samples/perc/close/" + inst, pattern), 1.0)]


U_KICK = (("in", 1.0, 0.0), ("out", 0.8, 0.0), ("oh", 0.5, 0.0))
U_SNARE = (("top", 1.0, 0.0), ("btm", 0.6, 0.0), ("ohl", 0.5, -0.8), ("ohr", 0.5, 0.8))
U_HAT = (("cl", 1.0, 0.0), ("ohl", 0.45, -0.8), ("ohr", 0.45, 0.8))
U_CYM = (("cl", 0.9, 0.0), ("oh", 0.7, 0.0))
S_SNARE = (("_top", 1.0), ("_btm", 0.6))
V_KICK = (("kickmic", 1.0), ("mid", 0.6), ("oh", 0.4))
V_SNARE = (("snaremic", 1.0), ("mid", 0.6), ("oh", 0.5))
V_KIT = (("mid", 1.0), ("oh", 0.7))

# Longest a sound may ring, in seconds, by category.
LENGTH = {"Kicks": 1.5, "Snares": 2.0, "Claps": 1.5, "Hi-Hats": 2.5, "Cymbals": 5.0, "Percussion": 3.0}


def sound(category, name, sources, seconds=None):
    return {"category": category, "name": name, "seconds": seconds or LENGTH[category], "sources": sources}


def brk(name, bpm, swing, voices, tracks):
    return {"name": "%s %d" % (name, bpm), "bpm": bpm, "swing": swing, "voices": voices, "tracks": tracks}


# The same 16th-step patterns the built-in library uses for its synthesised breaks:
# x hit, X accent, o ghost note, . rest.
AMEN = [
    ["k", "x.x.......xx.... x.x.......xx.... x.x.......x..... ..xx......x....."],
    ["s", "....x..o.o..x..o ....x..o.o..x..o ....x..o.o....x. .o..x..o.o....x."],
    ["r", "x.x.x.x.x.x.x.x. x.x.x.x.x.x.x.x. x.x.x.x.x.x.x.x. x.x.x.x.x......."],
    ["c", "................ ................ ................ ..........x....."],
]
THINK = [
    ["k", "x.....x.x.x..x.. x.x...x...x....."],
    ["s", "....x.......x... ....x..o....x..o"],
    ["h", "x.x.x.x.x.x.x... x.x.x.x.x.x.x.x."],
    ["oh", "..............x. ................"],
]
APACHE = [
    ["k", "x.....x...x..x.. x.....x...x....."],
    ["s", "....x.......x... ....x.......x..x"],
    ["h", "x.xxx.xxx.xxx.xx x.xxx.xxx.xxx.xx"],
    ["b", "..x...x...x.x... ..x...x...x.x.x."],
    ["cg", "x.......x....... x.......x.x....."],
]
TWO_STEP = [
    ["k", "x.........x..... x.........x....."],
    ["s", "....x.......x... ....x.......x..o"],
    ["h", "o.x.o.x.o.x.o.x. o.x.o.x.o.x.o.x."],
]
ROLLER = [
    ["k", "x..x......x..x.. x..x......x....."],
    ["s", "....x.o..o.ox..o ....x.o..o..x.ox"],
    ["h", "x.x.x.x.x.x.x.x. x.x.x.x.x.x.x.x."],
]
HALFTIME = [
    ["k", "x.....x...x..... x.....x...x...x."],
    ["s", "........x....... ........x......."],
    ["h", "x.x.x.x.x.x.x.x. x.x.x.x.x.x.x.x."],
]
FUNKY = [
    ["k", "x.x...x...x..x.. x.x...x...x..x.."],
    ["s", "....x..o.o..x..o ....x..o.o..x..o"],
    ["h", "xxxxxxx.xxxxxxxx xxxxxxx.xxxxxxxx"],
    ["oh", ".......x........ .......x........"],
]
SHUFFLE = [
    ["k", "x.........x..... x.........x....."],
    ["s", "....x.......x... ....x.......x..."],
    ["h", "x.ox.ox.x.ox.ox. x.ox.ox.x.ox.ox."],
    ["r", "x...x...x...x... x...x...x...x..."],
]

packs = []

# ---- Unruly Drums: a loud, slightly broken rock kit ---------------------------------
voices = {
    "k": "Kicks/Unruly Kick Dirty", "s": "Snares/Unruly Snare 14", "h": "Hi-Hats/Unruly Hat Closed",
    "oh": "Hi-Hats/Unruly Hat Open", "r": "Cymbals/Unruly Ride", "c": "Cymbals/Unruly Crash",
    "b": "Percussion/Unruly Woodblock High", "cg": "Percussion/Unruly Woodblock Low",
}
packs.append({
    "name": "Unruly Drums",
    "by": "Karoryfer Samples",
    "url": "https://github.com/sfzinstruments/karoryfer.unruly-drums",
    "sounds": [
        sound("Kicks", "Unruly Kick Clean", unruly("k20", "kick_clean", U_KICK)),
        sound("Kicks", "Unruly Kick Dirty", unruly("k20", "kick_dirty", U_KICK)),
        sound("Kicks", "Unruly Kick Dirtiest", unruly("k20", "kick_dirtiest", U_KICK)),
        sound("Snares", "Unruly Snare 13", unruly("s13", "s13_center", U_SNARE)),
        sound("Snares", "Unruly Snare 13 Rimshot", unruly("s13", "s13_rimshot", U_SNARE)),
        sound("Snares", "Unruly Snare 13 Side Stick", unruly("s13", "s13_sstick", U_SNARE)),
        sound("Snares", "Unruly Snare 14", unruly("s14", "s14_center", U_SNARE)),
        sound("Snares", "Unruly Snare 14 Rimshot", unruly("s14", "s14_rimshot", U_SNARE)),
        sound("Snares", "Unruly Snare 14 Edge", unruly("s14", "s14_edge", U_SNARE)),
        sound("Snares", "Unruly Snare 14 Brush", unruly("s14", "s14_b_center", U_SNARE)),
        sound("Snares", "Unruly Snare 22", unruly("s22", "s22_center", U_SNARE)),
        sound("Snares", "Unruly Snare 22 Edge", unruly("s22", "s22_edge", U_SNARE)),
        sound("Hi-Hats", "Unruly Hat Tight", unruly("h14", "hh_tight_tip", U_HAT)),
        sound("Hi-Hats", "Unruly Hat Closed", unruly("h14", "hh_closed_tip", U_HAT)),
        sound("Hi-Hats", "Unruly Hat Closed Shank", unruly("h14", "hh_closed_shank", U_HAT)),
        sound("Hi-Hats", "Unruly Hat Loose", unruly("h14", "hh_loose_tip", U_HAT)),
        sound("Hi-Hats", "Unruly Hat Quarter Open", unruly("h14", "hh_quarter_tip", U_HAT)),
        sound("Hi-Hats", "Unruly Hat Half Open", unruly("h14", "hh_half_tip", U_HAT)),
        sound("Hi-Hats", "Unruly Hat Open", unruly("h14", "hh_open_tip", U_HAT)),
        sound("Hi-Hats", "Unruly Hat Open Shank", unruly("h14", "hh_open_shank", U_HAT)),
        sound("Hi-Hats", "Unruly Hat Foot", unruly("h14", "hh_footchik", U_HAT)),
        sound("Hi-Hats", "Unruly Hat Foot Splash", unruly("h14", "hh_footspl", U_HAT)),
        sound("Cymbals", "Unruly Crash", unruly("c16", "cr_edge", U_CYM)),
        sound("Cymbals", "Unruly Crash Bow", unruly("c16", "cr_bow", U_CYM)),
        sound("Cymbals", "Unruly Crash Choke", unruly("c16", "cr_choke", U_CYM)),
        sound("Cymbals", "Unruly Ride", unruly("r20", "ride_bow", U_CYM)),
        sound("Cymbals", "Unruly Ride Bell", unruly("r20", "ride_bell", U_CYM)),
        sound("Cymbals", "Unruly Ride Edge", unruly("r20", "ride_edge", U_CYM)),
        sound("Percussion", "Unruly Woodblock High", unruly("wb", "woodblock_hi", U_CYM)),
        sound("Percussion", "Unruly Woodblock Low", unruly("wb", "woodblock_lo", U_CYM)),
        sound("Percussion", "Unruly Snares Off 13", unruly("s13", "s13_tom_dirty", U_SNARE)),
        sound("Percussion", "Unruly Snares Off 22", unruly("s22", "s22_tom_clean", U_SNARE)),
    ],
    "breaks": [
        brk("Unruly Amen-style", 170, 0.0, voices, AMEN),
        brk("Unruly Think-style", 170, 0.08, voices, THINK),
        brk("Unruly Two-Step", 174, 0.0, voices, TWO_STEP),
        brk("Unruly Jungle Roller", 165, 0.0, voices, ROLLER),
        brk("Unruly Halftime", 170, 0.0, voices, HALFTIME),
    ],
})

# ---- Swirly Drums: a dry kit with hand percussion -------------------------------------
voices = {
    "k": "Kicks/Swirly Kick", "s": "Snares/Swirly Snare", "h": "Hi-Hats/Swirly Hat Closed",
    "oh": "Hi-Hats/Swirly Hat Open", "r": "Cymbals/Swirly Ride", "c": "Cymbals/Swirly Crash",
    "b": "Percussion/Swirly Bongo High", "cg": "Percussion/Swirly Bongo Low",
}
packs.append({
    "name": "Swirly Drums",
    "by": "Karoryfer Samples",
    "url": "https://github.com/sfzinstruments/karoryfer.swirly-drums",
    "sounds": [
        sound("Kicks", "Swirly Kick", swirly("kick_buzz", "buzz_kick_with_snare_buzz")),
        sound("Kicks", "Swirly Marching Kick", swirly("marching_kick", "marching_kick", (("_beater", 1.0), ("_reso", 0.7)))),
        sound("Kicks", "Swirly Cajon Kick", swirly("cajon_kick", "cajon_kick", (("_front", 1.0), ("_rear", 0.6)))),
        sound("Snares", "Swirly Snare", swirly("snare_main", "snare_hit", S_SNARE)),
        sound("Snares", "Swirly Snare Edge", swirly("snare_edge", "snare_edge", S_SNARE)),
        sound("Snares", "Swirly Snare Dig", swirly("snare_dig", "snare_dig", S_SNARE)),
        sound("Hi-Hats", "Swirly Hat Tight", swirly("hat_tight", "hh_tight")),
        sound("Hi-Hats", "Swirly Hat Closed", swirly("hat_closed", "hh_closed")),
        sound("Hi-Hats", "Swirly Hat Quarter Open", swirly("hat_quarter", "hh_quarter")),
        sound("Hi-Hats", "Swirly Hat Half Open", swirly("hat_half", "hh_half")),
        sound("Hi-Hats", "Swirly Hat Open", swirly("hat_open", "hh_open")),
        sound("Hi-Hats", "Swirly Hat Sizzle", swirly("hat_sizzling", "hh_sizzling")),
        sound("Hi-Hats", "Swirly Hat Foot", swirly("hat_foot", "hh_foot")),
        sound("Hi-Hats", "Swirly Hat Foot Splash", swirly("hat_footsplash", "hh_footsplash")),
        sound("Cymbals", "Swirly Crash", swirly("crash", "crash")),
        sound("Cymbals", "Swirly Broken Crash", swirly("crash_broken", "crash")),
        sound("Cymbals", "Swirly China", swirly("china", "china")),
        sound("Cymbals", "Swirly Splash", swirly("splash", "splash")),
        sound("Cymbals", "Swirly Broken Splash", swirly("splash_broken", "splash")),
        sound("Cymbals", "Swirly Ride", swirly("ride", "ride")),
        sound("Percussion", "Swirly Tom 1 High", swirly("tom_vhi", "tom_vhi")),
        sound("Percussion", "Swirly Tom 2", swirly("tom_mhi", "tom_mhi")),
        sound("Percussion", "Swirly Tom 3", swirly("tom_xmed", "tom_xmed")),
        sound("Percussion", "Swirly Tom 4", swirly("tom_mlow", "tom_mlow")),
        sound("Percussion", "Swirly Tom 5 Floor", swirly("tom_floor", "tom_floor")),
        sound("Percussion", "Swirly Darbouka", swirly("darbouka", "darbouka")),
        sound("Percussion", "Swirly Djembe", swirly("djembe", "djembe")),
        sound("Percussion", "Swirly Bongo High", swirly("hbongo", "hbongo")),
        sound("Percussion", "Swirly Bongo Low", swirly("lbongo", "lbongo")),
        sound("Percussion", "Swirly Cowbell", swirly("cowbell", "cowbell")),
    ],
    "breaks": [
        brk("Swirly Apache-style", 172, 0.05, voices, APACHE),
        brk("Swirly Funky-style", 100, 0.1, voices, FUNKY),
        brk("Swirly Liquid Shuffle", 174, 0.12, voices, SHUFFLE),
        brk("Swirly Two-Step", 174, 0.0, voices, TWO_STEP),
    ],
})

# ---- Frankensnare: snares of every size, hand claps, tambourine -----------------------
snares = [
    ("10", "10_basic", "sn"), ("12", "12_basic", "sn"), ("13", "13b_basic", "sn"), ("13 Rimshot", "13b_rimshot", "sn"),
    ("13 Tambourine", "13b_tamb", "sn"), ("14 A", "14a_basic", "sn"), ("14 A Rimshot", "14a_rimshot", "sr"),
    ("14 A Side Stick", "14a_basic", "ss"), ("14 D", "14d_basic", "sn"), ("14 P", "14p_basic", "sn"),
    ("14 P Muted", "14p_muted", "sm"), ("14 S", "14s_basic", "sn"), ("14 T", "14t_basic", "sn"),
    ("16", "16_basic", "sn"), ("16 Side Stick", "16_basic", "ss"), ("18 Prepared", "18_prep", "sn"),
    ("20", "20_basic", "sn"), ("20 M", "20m_basic", "sn"), ("20 M Tight", "20m_tight", "sn"),
    ("22", "22_basic", "sn"), ("22 Spring", "22_spring", "sn"), ("Ziggurat 1", "ziggurat_lev1", "sn"),
    ("Ziggurat 2", "ziggurat_lev2", "sn"),
]
packs.append({
    "name": "Frankensnare",
    "by": "Karoryfer Samples",
    "url": "https://github.com/sfzinstruments/karoryfer.frankensnare",
    "sounds": [sound("Snares", "Franken " + label, franken(inst, art)) for label, inst, art in snares] + [
        sound("Claps", "Franken Clap 1", franken_one("xtra_claps", r"clap_person1_rr1")),
        sound("Claps", "Franken Clap 2", franken_one("xtra_claps", r"clap_person2_rr1")),
        sound("Claps", "Franken Clap 3", franken_one("xtra_claps", r"clap_person3_rr1")),
        sound("Claps", "Franken Clap Stack 1", franken_one("xtra_claps", r"clap_stack_rr1")),
        sound("Claps", "Franken Clap Stack 2", franken_one("xtra_claps", r"clap_stack_rr2")),
        sound("Claps", "Franken Clap Stack 3", franken_one("xtra_claps", r"clap_stack_rr3")),
        sound("Percussion", "Franken Tambourine", franken_one("xtra_tamb", r"tamb_vl(\d+)_rr1")),
    ],
    "breaks": [],
})

# ---- Virtuosity Drums: a jazz kit and orchestral / latin percussion -------------------
voices = {
    "k": "Kicks/Jazz Kick", "s": "Snares/Jazz Snare", "h": "Hi-Hats/Jazz Hat Closed",
    "oh": "Hi-Hats/Jazz Hat Open", "r": "Cymbals/Jazz Ride", "c": "Cymbals/Jazz Crash",
    "b": "Percussion/Bongo High", "cg": "Percussion/Conga",
}
packs.append({
    "name": "Virtuosity Drums",
    "by": "Versilian Studios",
    "url": "https://github.com/sfzinstruments/virtuosity_drums",
    "sounds": [
        sound("Kicks", "Jazz Kick", virtuo("kick", "kick_snoff", V_KICK)),
        sound("Kicks", "Jazz Kick Buzz", virtuo("kick", "kick_snon", V_KICK)),
        sound("Snares", "Jazz Snare", virtuo("snare", "snare_center", V_SNARE)),
        sound("Snares", "Jazz Snare Rimshot", virtuo("snare", "snare_rimshot", V_SNARE)),
        sound("Snares", "Jazz Snare Cross Stick", virtuo("snare", "snare_crossstick", V_SNARE)),
        sound("Snares", "Jazz Snare Buzz", virtuo("snare", "snare_buzz", V_SNARE)),
        sound("Snares", "Jazz Snare Flam", virtuo("snare", "snare_flam", V_SNARE)),
        sound("Snares", "Jazz Snare Muted", virtuo("snare", "snare_muted", V_SNARE)),
        sound("Hi-Hats", "Jazz Hat Closed", virtuo("hh", "hh_closed", V_KIT)),
        sound("Hi-Hats", "Jazz Hat Half Open", virtuo("hh", "hh_half", V_KIT)),
        sound("Hi-Hats", "Jazz Hat Open", virtuo("hh", "hh_open", V_KIT), 3.5),
        sound("Hi-Hats", "Jazz Hat Pedal", virtuo("hh", "hh_pedal", V_KIT)),
        sound("Cymbals", "Jazz Crash", virtuo("crash", "crash_crash", V_KIT)),
        sound("Cymbals", "Jazz Sizzle Crash", virtuo("crash", "crash_sizzle", V_KIT)),
        sound("Cymbals", "Jazz Ride", virtuo("ride", "ride_ride", V_KIT)),
        sound("Cymbals", "Jazz Ride Bell", virtuo("ride", "ride_bell", V_KIT)),
        sound("Cymbals", "Jazz Flat Ride", virtuo("flatride", "flatride_ride", V_KIT)),
        sound("Percussion", "Jazz Tom High", virtuo("htom", "htom_center", V_KIT)),
        sound("Percussion", "Jazz Tom Low", virtuo("ltom", "ltom_center", V_KIT)),
        sound("Percussion", "Agogo High", perc("agogo", r"Agogo_High_v(\d+)_rr1_Close")),
        sound("Percussion", "Agogo Low", perc("agogo", r"Agogo_Low_v(\d+)_rr1_Close")),
        sound("Percussion", "Bell Tree", perc("belltree", r"BellTree_Stroke_rr1_Close"), 5.0),
        sound("Percussion", "Bongo High", perc("bongoh", r"BongoH_Hit1_v(\d+)_rr1_Close")),
        sound("Percussion", "Bongo Low", perc("bongol", r"BongoL_Hit1_v(\d+)_rr1_Close")),
        sound("Percussion", "Cabasa", perc("cabasa", r"Cabasa1_Rub_v(\d+)_rr1_Close")),
        sound("Percussion", "Claves", perc("claves", r"Claves1_Hit_v(\d+)_rr1_Close")),
        sound("Percussion", "Conga High", perc("conga", r"Conga_17_HitHM1_71_127_rr1_Close")),
        sound("Percussion", "Conga", perc("conga", r"Conga_22_HitN_101_127_rr1_Close")),
        sound("Percussion", "Cowbell", perc("cowbell", r"Cowbell1_Normal_v(\d+)_rr1_Close")),
        sound("Percussion", "Guiro Fast", perc("guiro", r"Guiro_Fast(?:_rr1)?_Close")),
        sound("Percussion", "Guiro Slow", perc("guiro", r"Guiro_Med(?:_rr1)?_Close")),
        sound("Percussion", "Shaker Down", perc("shaker", r"LShaker_Shake1D(?:_rr1)?_Close")),
        sound("Percussion", "Shaker Up", perc("shaker", r"LShaker_Shake1U(?:_rr1)?_Close")),
        sound("Percussion", "Sleigh Bells", perc("sleighbells", r"Sleighbells_Hit(?:_rr1)?_Close")),
        sound("Percussion", "Tambourine Shake", perc("tambourine", r"Tamb1_Shake(?:_rr1)?_Close")),
        sound("Percussion", "Triangle", perc("triangle", r"Triangle1_Hit_v(\d+)(?:_rr1)?_Close"), 5.0),
        sound("Percussion", "Triangle Muted", perc("triangle", r"Triangle1_HitM_v(\d+)(?:_rr1)?_Close")),
        sound("Percussion", "Tumba", perc("tumba", r"Tumba_24_HitN_101_127_rr1_Close")),
        sound("Percussion", "Vibraslap", perc("vibraslap", r"Vibraslap1_1_Hit_71_127(?:_rr1)?_Close")),
        sound("Percussion", "Whistle Short", perc("whistle", r"Close_BallWhistle_Short-001")),
        sound("Percussion", "Whistle Long", perc("whistle", r"Close_BallWhistle_Long-001")),
    ],
    "breaks": [
        brk("Jazz Amen-style", 170, 0.0, voices, AMEN),
        brk("Jazz Funky-style", 100, 0.1, voices, FUNKY),
        brk("Jazz Liquid Shuffle", 174, 0.12, voices, SHUFFLE),
        brk("Jazz Apache-style", 172, 0.05, voices, APACHE),
    ],
})

unique = {}
for pack in packs:
    for s in pack["sounds"]:
        for f in s["sources"]:
            unique[(f["repo"], f["path"])] = f["size"]
json.dump({"license": "CC0-1.0", "packs": packs}, open(OUT, "w", encoding="utf-8"), indent=1)
for pack in packs:
    print("%-18s %3d sounds, %d breaks" % (pack["name"], len(pack["sounds"]), len(pack["breaks"])))
print("source files: %d, %.1f MB" % (len(unique), sum(unique.values()) / 1048576.0))
