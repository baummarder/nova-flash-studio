import json, uuid

def uid():
    return str(uuid.uuid4())

# M18 demo: AS3 `switch` statement (new in the compiler) with strict
# equality, fall-through and default, plus a gradient path so the SWF
# also exercises the M17/M18 vector pipeline under Ruffle.
gradient_card = {
    "id": uid(), "name": "GradientCard",
    "kind": {"kind": "path", "width": 200.0, "height": 100.0, "paths": [{
        "fill": {"r": 255, "g": 255, "b": 255, "a": 255},
        "gradient": {
            "kind": "linear", "angle": 0.0,
            "stops": [
                {"position": 0.0, "color": {"r": 80, "g": 200, "b": 255, "a": 255}},
                {"position": 1.0, "color": {"r": 200, "g": 80, "b": 255, "a": 255}},
            ],
        },
        "commands": [
            {"kind": "move_to", "x": -100.0, "y": -50.0},
            {"kind": "line_to", "x": 100.0, "y": -50.0},
            {"kind": "line_to", "x": 100.0, "y": 50.0},
            {"kind": "line_to", "x": -100.0, "y": 50.0},
            {"kind": "line_to", "x": -100.0, "y": -50.0},
        ],
    }]},
    "transform": {"x": 320.0, "y": 120.0, "scale_x": 1.0, "scale_y": 1.0, "rotation": 0.0},
    "fill": {"r": 255, "g": 255, "b": 255, "a": 255},
}

src = r'''package {
    import flash.display.MovieClip;
    import flash.events.Event;

    public dynamic class Main extends MovieClip {
        private var frame:int = 0;

        public function Main() {
            addEventListener("enterFrame", onTick);
        }

        private function probe(value:int):int {
            var out:int = 0;
            switch (value) {
                case 1:
                    out += 10;      // fall-through into case 2
                case 2:
                    out += 20;
                    break;
                case 3, 4:
                    out += 30;
                    break;
                default:
                    out = -1;
            }
            return out;
        }

        private function onTick(e:Event):void {
            frame++;
            if (frame == 5) {
                trace("SW1=" + probe(1));   // 30 (fall-through 10+20)
                trace("SW2=" + probe(2));   // 20
                trace("SW3=" + probe(3));   // 30
                trace("SW4=" + probe(4));   // 30
                trace("SW5=" + probe(9));   // -1 (default)
            }
            if (frame == 21) {
                trace("M18DONE");
            }
        }
    }
}
'''

proj = {"id": uid(), "metadata": {"name": "M18 Switch & Compatibility", "author": "", "description": "", "createdUnix": 0, "modifiedUnix": 0},
        "scenes": [{"id": uid(), "name": "Szene 1", "width": 640.0, "height": 360.0, "background": {"r": 18, "g": 20, "b": 26, "a": 255}, "fps": 24.0, "layers": [{"id": uid(), "name": "Ebene 1", "visible": True, "locked": False, "kind": "normal", "frames": [{"keyframe": True, "objects": [gradient_card]}]}], "game": None}],
        "symbols": [], "assets": [], "tilesets": [],
        "scripts": [{"id": uid(), "name": "Main.as", "path": "scripts/Main.as", "source": src, "provenance": "original_source"}],
        "export": {"target": "avm2", "swfVersion": 13, "compression": "zlib", "includeScripts": True, "exportAllScenes": False}, "activeScene": None}
proj["activeScene"] = proj["scenes"][0]["id"]
env = {"format": "nova-flash-studio", "formatVersion": 4, "project": proj}
open("samples/m18-switch-compat.nova", "w", encoding="utf-8").write(json.dumps(env, indent=1))
print("written")
