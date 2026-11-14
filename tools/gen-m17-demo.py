import json, uuid

def uid():
    return str(uuid.uuid4())

# Rig like M15: Torso -> Oberarm -> Unterarm, plus a skinned "ArmMesh"
# path object whose vertices are weighted across all three bones.
BONES = [
    {"name": "Torso", "x": 0.0, "y": 0.0, "length": 30.0, "parent": None},
    {"name": "Oberarm", "x": 30.0, "y": 0.0, "length": 40.0, "parent": "Torso"},
    {"name": "Unterarm", "x": 40.0, "y": 0.0, "length": 40.0, "parent": "Oberarm"},
]
bone_ids = {b["name"]: uid() for b in BONES}

def rect_obj(name, x, y, w, h, fill, bone=None):
    return {
        "id": uid(), "name": name,
        "kind": {"kind": "rect", "width": w, "height": h, "corner_radius": 0.0},
        "transform": {"x": x, "y": y, "scale_x": 1.0, "scale_y": 1.0, "rotation": 0.0},
        "fill": fill, "bone": bone,
    }

# Skinned mesh: hexagonal strip along the arm (local coords, object at
# x=55 so it spans all three bone joints at world x=0/30/70).
def w(bone, weight):
    return {"bone": bone_ids[bone], "weight": weight}

mesh_commands = [
    {"kind": "move_to", "x": -40.0, "y": -9.0},
    {"kind": "line_to", "x": 0.0, "y": -11.0},
    {"kind": "line_to", "x": 40.0, "y": -9.0},
    {"kind": "line_to", "x": 40.0, "y": 9.0},
    {"kind": "line_to", "x": 0.0, "y": 11.0},
    {"kind": "line_to", "x": -40.0, "y": 9.0},
    {"kind": "line_to", "x": -40.0, "y": -9.0},
]
# 7 skinning points: left column -> Torso, middle -> Oberarm, right -> Unterarm.
mesh_weights = [
    [w("Torso", 1.0)],
    [w("Oberarm", 1.0)],
    [w("Unterarm", 1.0)],
    [w("Unterarm", 1.0)],
    [w("Oberarm", 1.0)],
    [w("Torso", 1.0)],
    [w("Torso", 1.0)],
]
mesh_obj = {
    "id": uid(), "name": "ArmMesh",
    "kind": {"kind": "path", "width": 80.0, "height": 22.0, "paths": [{
        "fill": {"r": 255, "g": 220, "b": 80, "a": 255},
        "commands": mesh_commands,
        "skin": {"weights": mesh_weights},
    }]},
    "transform": {"x": 55.0, "y": 0.0, "scale_x": 1.0, "scale_y": 1.0, "rotation": 0.0},
    "fill": {"r": 255, "g": 220, "b": 80, "a": 255},
}

objects = [
    rect_obj("TorsoSegment", 0.0, -40.0, 30.0, 80.0, {"r": 90, "g": 140, "b": 220, "a": 255}, bone_ids["Torso"]),
    rect_obj("OberarmSegment", 20.0, -12.0, 40.0, 24.0, {"r": 240, "g": 160, "b": 60, "a": 255}, bone_ids["Oberarm"]),
    rect_obj("UnterarmSegment", 20.0, -10.0, 40.0, 20.0, {"r": 120, "g": 200, "b": 120, "a": 255}, bone_ids["Unterarm"]),
    mesh_obj,
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
    ],
    "active_clip": "Schwung",
}

# Gradient card on the main scene.
gradient_card = {
    "id": uid(), "name": "GradientCard",
    "kind": {"kind": "path", "width": 160.0, "height": 90.0, "paths": [{
        "fill": {"r": 255, "g": 255, "b": 255, "a": 255},
        "gradient": {
            "kind": "linear", "angle": 35.0,
            "stops": [
                {"position": 0.0, "color": {"r": 255, "g": 80, "b": 40, "a": 255}},
                {"position": 0.55, "color": {"r": 255, "g": 200, "b": 60, "a": 255}},
                {"position": 1.0, "color": {"r": 40, "g": 160, "b": 255, "a": 255}},
            ],
        },
        "commands": [
            {"kind": "move_to", "x": -80.0, "y": -45.0},
            {"kind": "line_to", "x": 80.0, "y": -45.0},
            {"kind": "line_to", "x": 80.0, "y": 45.0},
            {"kind": "line_to", "x": -80.0, "y": 45.0},
            {"kind": "line_to", "x": -80.0, "y": -45.0},
        ],
    }]},
    "transform": {"x": 320.0, "y": 60.0, "scale_x": 1.0, "scale_y": 1.0, "rotation": 0.0},
    "fill": {"r": 255, "g": 255, "b": 255, "a": 255},
}

src = r'''package {
    import flash.display.MovieClip;
    import flash.display.Sprite;
    import flash.events.Event;

    public dynamic class Main extends MovieClip {
        private var frame:int = 0;

        public function Main() {
            addEventListener("enterFrame", onTick);
        }

        private function onTick(e:Event):void {
            frame++;
            if (frame == 21) {
                trace("M17DONE");
            }
        }
    }
}
'''

proj = {"id": uid(), "metadata": {"name": "M17 Vector & Skinning", "author": "", "description": "", "createdUnix": 0, "modifiedUnix": 0},
        "scenes": [{"id": uid(), "name": "Szene 1", "width": 640.0, "height": 360.0, "background": {"r": 18, "g": 20, "b": 26, "a": 255}, "fps": 24.0, "layers": [{"id": uid(), "name": "Ebene 1", "visible": True, "locked": False, "kind": "normal", "frames": [{"keyframe": True, "objects": [
            gradient_card,
            {"id": uid(), "name": "Held", "kind": {"kind": "symbol", "symbol": symbol["id"]},
             "transform": {"x": 320.0, "y": 240.0, "scale_x": 1.0, "scale_y": 1.0, "rotation": 0.0},
             "fill": {"r": 255, "g": 255, "b": 255, "a": 255}},
        ]}]}], "game": None}],
        "symbols": [symbol], "assets": [], "tilesets": [],
        "scripts": [{"id": uid(), "name": "Main.as", "path": "scripts/Main.as", "source": src, "provenance": "original_source"}],
        "export": {"target": "avm2", "swfVersion": 13, "compression": "zlib", "includeScripts": True, "exportAllScenes": False}, "activeScene": None}
proj["activeScene"] = proj["scenes"][0]["id"]
env = {"format": "nova-flash-studio", "formatVersion": 4, "project": proj}
open("samples/m17-vector-skinning.nova", "w", encoding="utf-8").write(json.dumps(env, indent=1))
print("written")
