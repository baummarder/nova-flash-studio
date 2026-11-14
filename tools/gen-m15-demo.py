import json, uuid

def uid():
    return str(uuid.uuid4())

# Rig: root "Torso" bone at (0,0); "Oberarm" child at (30,0); "Unterarm" at (70,0).
# Bound parts are rects whose local transform is the bind offset from the
# bone joint (centre-based, bone direction = +x).
BONES = [
    {"name": "Torso", "x": 0.0, "y": 0.0, "length": 30.0, "parent": None},
    {"name": "Oberarm", "x": 30.0, "y": 0.0, "length": 40.0, "parent": "Torso"},
    {"name": "Unterarm", "x": 40.0, "y": 0.0, "length": 40.0, "parent": "Oberarm"},
]

def rect_obj(name, x, y, w, h, fill, bone=None):
    return {
        "id": uid(), "name": name,
        "kind": {"kind": "rect", "width": w, "height": h, "corner_radius": 0.0},
        "transform": {"x": x, "y": y, "scale_x": 1.0, "scale_y": 1.0, "rotation": 0.0},
        "fill": fill, "bone": bone,
    }

# Deterministic bone ids so the .nova is reproducible enough for tests.
bone_ids = {b["name"]: uid() for b in BONES}

objects = [
    # Torso segment: centred on the root bone, spanning -15..+15 along x.
    rect_obj("TorsoSegment", 0.0, -40.0, 30.0, 80.0, {"r": 90, "g": 140, "b": 220, "a": 255}, bone_ids["Torso"]),
    # Upper arm: 40 long, 24 thick, offset so it starts at the joint.
    rect_obj("OberarmSegment", 20.0, -12.0, 40.0, 24.0, {"r": 240, "g": 160, "b": 60, "a": 255}, bone_ids["Oberarm"]),
    # Forearm: 40 long, 20 thick.
    rect_obj("UnterarmSegment", 20.0, -10.0, 40.0, 20.0, {"r": 120, "g": 200, "b": 120, "a": 255}, bone_ids["Unterarm"]),
]

symbol = {
    "id": uid(), "name": "Held", "kind": "movie_clip",
    "width": 200.0, "height": 200.0,
    "fill": {"r": 0, "g": 0, "b": 0, "a": 0}, "corner_radius": 0.0,
    "timeline": {
        "id": uid(), "name": "Held", "width": 200.0, "height": 200.0,
        "background": {"r": 0, "g": 0, "b": 0, "a": 0}, "fps": 24.0,
        "layers": [{"id": uid(), "name": "Teile", "visible": True, "locked": False,
                    "kind": "normal",
                    "frames": [{"keyframe": True, "objects": objects}] + [{}] * 23}],
    },
    "skeleton": {"bones": [
        {"id": bone_ids[b["name"]], "name": b["name"], "parent": (bone_ids[b["parent"]] if b["parent"] else None),
         "x": b["x"], "y": b["y"], "length": b["length"], "rotation": 0.0}
        for b in BONES
    ]},
    "bone_clips": [
        {"name": "Schwung", "frames": 24, "looping": True, "tracks": [
            {"bone": bone_ids["Oberarm"], "keys": [
                {"frame": 0, "rotation": 0.0}, {"frame": 12, "rotation": -70.0}, {"frame": 24, "rotation": 0.0}]},
            {"bone": bone_ids["Unterarm"], "keys": [
                {"frame": 0, "rotation": 20.0}, {"frame": 12, "rotation": 60.0}, {"frame": 24, "rotation": 20.0}]},
        ]},
        {"name": "Greifen", "frames": 24, "looping": True, "tracks": [
            {"bone": bone_ids["Unterarm"], "ik": [
                {"frame": 0, "x": 80.0, "y": 30.0, "poleX": 0.0, "poleY": 120.0},
                {"frame": 12, "x": 60.0, "y": -50.0, "poleX": 0.0, "poleY": -120.0},
                {"frame": 24, "x": 80.0, "y": 30.0, "poleX": 0.0, "poleY": 120.0}]},
        ]},
    ],
    "active_clip": "Schwung",
}

src = r'''package {
    import flash.display.MovieClip;
    import flash.display.Sprite;
    import flash.events.Event;

    public dynamic class Main extends MovieClip {
        private var frame:int = 0;
        private var hero:Sprite;

        public function Main() {
            addEventListener("enterFrame", onTick);
        }

        private function onTick(e:Event):void {
            frame++;
            if (hero == null && numChildren > 0) {
                hero = getChildAt(numChildren - 1) as Sprite;
            }
            var probe:Boolean = (frame == 2 || frame == 8 || frame == 14 || frame == 20);
            if (probe) {
                if (hero != null) {
                var upper = hero.getChildByName("OberarmSegment");
                var fore = hero.getChildByName("UnterarmSegment");
                var torso = hero.getChildByName("TorsoSegment");
                trace("M15 f" + frame
                    + " torso=" + torso.x.toFixed(1) + "," + torso.y.toFixed(1) + ":" + torso.rotation.toFixed(1)
                    + " upper=" + upper.x.toFixed(1) + "," + upper.y.toFixed(1) + ":" + upper.rotation.toFixed(1)
                    + " fore=" + fore.x.toFixed(1) + "," + fore.y.toFixed(1) + ":" + fore.rotation.toFixed(1));
                }
            }
            if (frame == 21) {
                trace("M15DONE");
            }
        }
    }
}
'''

proj = {"id": uid(), "metadata": {"name": "M15 Skeletal Animation", "author": "", "description": "", "createdUnix": 0, "modifiedUnix": 0},
        "scenes": [{"id": uid(), "name": "Szene 1", "width": 640.0, "height": 360.0, "background": {"r": 18, "g": 20, "b": 26, "a": 255}, "fps": 24.0, "layers": [{"id": uid(), "name": "Ebene 1", "visible": True, "locked": False, "kind": "normal", "frames": [{"keyframe": True, "objects": [
            {"id": uid(), "name": "Held", "kind": {"kind": "symbol", "symbol": symbol["id"]},
             "transform": {"x": 320.0, "y": 180.0, "scale_x": 1.0, "scale_y": 1.0, "rotation": 0.0},
             "fill": {"r": 255, "g": 255, "b": 255, "a": 255}}
        ]}]}], "game": None}],
        "symbols": [symbol], "assets": [], "tilesets": [],
        "scripts": [{"id": uid(), "name": "Main.as", "path": "scripts/Main.as", "source": src, "provenance": "original_source"}],
        "export": {"target": "avm2", "swfVersion": 13, "compression": "zlib", "includeScripts": True, "exportAllScenes": False}, "activeScene": None}
proj["activeScene"] = proj["scenes"][0]["id"]
env = {"format": "nova-flash-studio", "formatVersion": 4, "project": proj}
open("samples/m15-skeletal-animation.nova", "w", encoding="utf-8").write(json.dumps(env, indent=1))
print("written")
