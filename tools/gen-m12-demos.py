#!/usr/bin/env python3
"""Generates the four M12 demo .nova projects under samples/.

Each demo embeds a Main.as that drives game.NovaGame and traces M12
assertion markers; tools/ruffle-runtime-test.py consumes those markers.
Run from the repo root:  python tools/gen-m12-demos.py
"""

import json
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SAMPLES = REPO / "samples"
NOVA_GAME_AS = Path.home() / "AppData" / "Local" / "Temp" / "NovaGame.as"
# Fallback: extract from gamekit.rs if the temp copy is stale/missing.
GAMEKIT_RS = REPO / "crates" / "nova-core" / "src" / "gamekit.rs"


def nova_game_source() -> str:
    if NOVA_GAME_AS.exists():
        return NOVA_GAME_AS.read_text(encoding="utf-8")
    import re
    src = GAMEKIT_RS.read_text(encoding="utf-8")
    m = re.search(r'const NOVA_GAME: &str = r#"(.*?)"#;', src, re.S)
    if not m:
        raise SystemExit("NOVA_GAME source not found")
    return m.group(1)


def uid() -> str:
    return str(uuid.uuid4())


def layer(name: str, objects: list, nframes: int = 1) -> dict:
    frames = []
    for i in range(nframes):
        f = {"keyframe": True}
        if i == 0 and objects:
            f["objects"] = objects
        elif objects:
            f["objects"] = [json.loads(json.dumps(o)) for o in objects]
        frames.append(f)
    return {
        "id": uid(),
        "name": name,
        "visible": True,
        "locked": False,
        "kind": "normal",
        "frames": frames,
    }


def rect(name, x, y, w, h, color, game=None):
    o = {
        "id": uid(),
        "name": name,
        "kind": {"kind": "rect", "width": w, "height": h, "corner_radius": 0.0},
        "transform": {"x": x, "y": y, "scale_x": 1.0, "scale_y": 1.0, "rotation": 0.0},
        "fill": {"r": color[0], "g": color[1], "b": color[2], "a": 255},
    }
    if game:
        o["game"] = game
    return o


def ellipse(name, x, y, w, h, color, game=None):
    o = {
        "id": uid(),
        "name": name,
        "kind": {"kind": "ellipse", "width": w, "height": h},
        "transform": {"x": x, "y": y, "scale_x": 1.0, "scale_y": 1.0, "rotation": 0.0},
        "fill": {"r": color[0], "g": color[1], "b": color[2], "a": 255},
    }
    if game:
        o["game"] = game
    return o


def collision(shape="rectangle", w=32.0, h=32.0, layer=1, mask=0xFFFFFFFF):
    c = {"shape": shape, "width": w, "height": h, "layer": layer, "mask": mask}
    if shape == "polygon":
        c["points"] = [[-16, -16], [16, -16], [16, 16], [-16, 16]]
    return c


def physics(body="static", gravity=1.0, friction=0.0, bounce=0.0, controlled=False):
    return {
        "body": body,
        "gravityScale": gravity,
        "friction": friction,
        "bounce": bounce,
        "controlled": controlled,
    }


def scene(name, objects, nframes=1, game=None, w=640.0, h=360.0, bg=(18, 20, 26)):
    return {
        "id": uid(),
        "name": name,
        "width": w,
        "height": h,
        "background": {"r": bg[0], "g": bg[1], "b": bg[2], "a": 255},
        "fps": 30.0,
        "layers": [layer("Ebene 1", objects, nframes)],
        "game": game,
    }


def script(path, source):
    return {
        "id": uid(),
        "name": Path(path).name,
        "path": path,
        "source": source,
        "provenance": "original_source",
    }


def gamekit_scripts():
    base = REPO / "crates" / "nova-core" / "src" / "gamekit.rs"
    import re
    src = base.read_text(encoding="utf-8")
    out = []
    for m in re.finditer(r'const (\w+): &str = r#"(.*?)"#;', src, re.S):
        const, body = m.group(1), m.group(2)
        # Map const name -> virtual path (order matches FILES).
        paths = {
            "INPUT": "scripts/game/Input.as",
            "COLLISION": "scripts/game/Collision.as",
            "PHYSICS": "scripts/game/Physics.as",
            "CAMERA": "scripts/game/Camera.as",
            "RANDOM": "scripts/game/Random.as",
            "NOVA_GAME": "scripts/game/NovaGame.as",
        }
        if const in paths:
            out.append(script(paths[const], body))
    return out


def project(scenes, scripts, active=0):
    return {
        "id": uid(),
        "name": "M12 Demo",
        "version": 4,
        "activeScene": scenes[active]["id"],
        "scenes": scenes,
        "scripts": scripts,
        "symbols": [],
        "assets": [],
        "export": {
            "target": "avm2",
            "swfVersion": 10,
            "compression": "zlib",
            "includeScripts": True,
            "exportAllScenes": False,
        },
        "metadata": {"name": "M12 Demo", "author": "", "description": "Mega Milestone 12 runtime parity demo", "createdUnix": 1763000000, "modifiedUnix": 1763000000},
    }


def write_demo(name, proj):
    path = SAMPLES / name
    path.write_text(json.dumps({"format": "nova-flash-studio", "formatVersion": 4, "project": proj}, indent=2), encoding="utf-8")
    print(f"wrote {path}")


# ---------------------------------------------------------------------------
# Demo 1: scene transitions (3 scenes, API + invalid target + return)
# ---------------------------------------------------------------------------

MAIN_TRANSITIONS = r'''package {
    import flash.display.MovieClip;
    import flash.events.Event;
    import game.NovaGame;

    /// M12 scene-transition demo. Three scenes; Enter is configured as the
    /// transition key in project metadata, and this script additionally
    /// exercises the public gotoScene API, an invalid target and a return
    /// to an earlier scene. Traces are consumed by the Ruffle harness.
    public dynamic class Main extends MovieClip {
        private var frame:int = 0;
        private var loads:int = 0;
        private var unloads:int = 0;
        private var transitions:int = 0;

        public function Main() {
            NovaGame.init(this);
            addEventListener("nova:sceneLoad", onLoad);
            addEventListener("nova:sceneUnload", onUnload);
            addEventListener("nova:sceneTransition", onTrans);
            addEventListener(Event.ENTER_FRAME, onTick);
        }

        private function onLoad(e:Event):void {
            loads++;
            trace("M12 load " + NovaGame.currentScene());
        }
        private function onUnload(e:Event):void {
            unloads++;
            trace("M12 unload " + NovaGame.eventInfo.a);
        }
        private function onTrans(e:Event):void {
            transitions++;
            trace("M12 trans " + NovaGame.eventInfo.a + "->" + NovaGame.eventInfo.b);
        }
        private function onTick(e:Event):void {
            frame++;
            if (frame == 30) {
                trace("M12 scenes=" + NovaGame.sceneCount());
                trace("M12 goto Level2 -> " + NovaGame.gotoScene("Level2"));
            }
            if (frame == 70) {
                trace("M12 goto Menu -> " + NovaGame.gotoScene("Menu"));
            }
            if (frame == 110) {
                trace("M12 goto Nope -> " + NovaGame.gotoScene("Nope"));
                trace("M12 still=" + NovaGame.currentScene());
            }
            if (frame == 150) {
                trace("M12 loads=" + loads + " unloads=" + unloads + " trans=" + transitions);
                trace("M12 ok loads=" + (loads == 2) + " unloads=" + (unloads == 2) + " trans=" + (transitions == 2));
                trace("M12DONE");
            }
        }
    }
}
'''

menu_scene = scene(
    "Menu",
    [
        rect("Title", 320, 120, 360, 80, (70, 120, 200)),
        rect("Hint", 320, 200, 300, 40, (90, 100, 140)),
    ],
    nframes=2,
    game={"transitions": [{"key": "Enter", "target": "Level1"}]},
    bg=(24, 18, 40),
)
level1_scene = scene(
    "Level1",
    [
        rect("Ground", 320, 330, 640, 30, (80, 90, 110),
             {"collision": collision(w=640, h=30), "physics": physics("static")}),
        rect("Player", 80, 300, 30, 40, (255, 210, 74),
             {"collision": collision(w=30, h=40), "physics": physics("dynamic", controlled=True)}),
        rect("Decor", 500, 150, 60, 60, (120, 160, 220)),
    ],
    nframes=2,
    game={"transitions": [{"key": "Space", "target": "Level2"}]},
)
level2_scene = scene(
    "Level2",
    [
        rect("Ground", 320, 330, 640, 30, (90, 80, 70),
             {"collision": collision(w=640, h=30), "physics": physics("static")}),
        rect("Player", 80, 300, 30, 40, (120, 220, 255),
             {"collision": collision(w=30, h=40), "physics": physics("dynamic", controlled=True)}),
        rect("Goal", 560, 300, 40, 60, (90, 220, 120)),
    ],
    nframes=2,
    game={"transitions": [{"key": "KeyR", "target": "Menu"}]},
    bg=(30, 26, 18),
)

write_demo(
    "m12-scene-transitions.nova",
    project(
        [menu_scene, level1_scene, level2_scene],
        [script("scripts/Main.as", MAIN_TRANSITIONS)] + gamekit_scripts(),
    ),
)

# ---------------------------------------------------------------------------
# Demo 2: event bus (multi-listener, removal, ordering, recursion guard)
# ---------------------------------------------------------------------------

MAIN_EVENTS = r'''package {
    import flash.display.MovieClip;
    import flash.events.Event;
    import game.NovaGame;

    /// M12 event-bus demo. A dynamic ball bounces on a static floor producing
    /// collisionEnter/collisionExit pairs; the script additionally exercises
    /// multiple listeners, stable ordering, listener removal, custom events
    /// and recursive-dispatch protection.
    public dynamic class Main extends MovieClip {
        private var frame:int = 0;
        private var enters:int = 0;
        private var exits:int = 0;
        private var orderA:int = 0;
        private var orderB:int = 0;
        private var customSeen:int = 0;
        private var removedSeen:int = 0;
        private var recursionDepth:int = 0;

        public function Main() {
            NovaGame.init(this);
            addEventListener("nova:collisionEnter", onEnterA);
            addEventListener("nova:collisionEnter", onEnterB);
            addEventListener("nova:collisionExit", onExit);
            addEventListener("nova:m12custom", onCustom);
            addEventListener("nova:m12recursive", onRecursive);
            addEventListener(Event.ENTER_FRAME, onTick);
        }

        private function onEnterA(e:Event):void {
            enters++;
            orderA++;
            trace("M12 enterA " + NovaGame.eventInfo.a + "<>" + NovaGame.eventInfo.b);
        }
        private function onEnterB(e:Event):void {
            orderB++;
            trace("M12 enterB " + NovaGame.eventInfo.a + "<>" + NovaGame.eventInfo.b);
        }
        private function onExit(e:Event):void {
            exits++;
            trace("M12 exit " + NovaGame.eventInfo.a + "<>" + NovaGame.eventInfo.b);
        }
        private function onCustom(e:Event):void {
            customSeen++;
            trace("M12 custom " + NovaGame.eventInfo.data);
        }
        private function onRecursive(e:Event):void {
            recursionDepth++;
            if (recursionDepth < 20) {
                NovaGame.emit("nova:m12recursive", recursionDepth);
            }
        }

        private function onTick(e:Event):void {
            frame++;
            if (frame == 25) {
                trace("M12 dbg " + NovaGame.debugInfo());
            }
            if (frame == 20) {
                // Custom event + payload.
                NovaGame.emit("nova:m12custom", "payload-1");
                // Listener removal: add then remove, emit, count must stay 0.
                addEventListener("nova:m12removed", onRemoved);
                removeEventListener("nova:m12removed", onRemoved);
                NovaGame.emit("nova:m12removed", null);
                // Recursive dispatch: listener re-emits its own event.
                NovaGame.emit("nova:m12recursive", 0);
            }
            if (frame == 140) {
                // Deterministic separation: teleport the ball back to the top
                // so the floor contact must end (exit) and later re-form.
                trace("M12 teleport=" + NovaGame.setBodyPosition("Ball", 320, 60));
            }
            if (frame == 60 || frame == 120 || frame == 180) {
                trace("M12 pos " + NovaGame.debugInfo());
            }
            if (frame == 240) {
                trace("M12 enters=" + enters + " exits=" + exits
                    + " orderA=" + orderA + " orderB=" + orderB
                    + " custom=" + customSeen + " removed=" + removedSeen
                    + " recDepth=" + recursionDepth
                    + " dropped=" + NovaGame.droppedEventCount());
                trace("M12 ok enter=" + (enters >= 2 && enters == orderA && enters == orderB)
                    + " exit=" + (exits >= 1)
                    + " custom=" + (customSeen == 1)
                    + " removed=" + (removedSeen == 0)
                    + " recurse=" + (NovaGame.droppedEventCount() > 0));
                trace("M12DONE");
            }
        }
        private function onRemoved(e:Event):void {
            removedSeen++;
        }
    }
}
'''

events_scene = scene(
    "Arena",
    [
        rect("Floor", 320, 345, 640, 30, (80, 90, 110),
             {"collision": collision(w=640, h=30), "physics": physics("static")}),
        ellipse("Ball", 320, 60, 24, 24, (255, 120, 60),
                {"collision": collision(shape="circle", w=24, h=24),
                 "physics": physics("dynamic", bounce=0.85)}),
        rect("WallL", 10, 180, 20, 200, (100, 100, 120),
             {"collision": collision(w=20, h=200), "physics": physics("static")}),
        rect("WallR", 630, 180, 20, 200, (100, 100, 120),
             {"collision": collision(w=20, h=200), "physics": physics("static")}),
    ],
    nframes=2,
    game={"world": {"width": 640.0, "height": 360.0, "spawn": [320.0, 60.0]}},
)

write_demo(
    "m12-event-bus.nova",
    project([events_scene], [script("scripts/Main.as", MAIN_EVENTS)] + gamekit_scripts()),
)

# ---------------------------------------------------------------------------
# Demo 3: collision parity (rect, circle, polygon, layer/mask)
# ---------------------------------------------------------------------------

MAIN_COLLISION = r'''package {
    import flash.display.MovieClip;
    import flash.events.Event;
    import game.NovaGame;

    /// M12 collision demo. Three dynamic bodies (rect, circle, convex
    /// polygon) fall onto static platforms; a layer/masked pair must NOT
    /// produce events. Traces feed the Ruffle runtime assertions.
    public dynamic class Main extends MovieClip {
        private var frame:int = 0;
        private var enters:int = 0;
        private var ghostHits:int = 0;
        private var seen:Object = {};

        public function Main() {
            NovaGame.init(this);
            addEventListener("nova:collisionEnter", onEnter);
            addEventListener(Event.ENTER_FRAME, onTick);
        }

        private function onEnter(e:Event):void {
            enters++;
            var key:String = NovaGame.eventInfo.a + "<>" + NovaGame.eventInfo.b;
            seen[key] = true;
            trace("M12 enter " + key);
            if (key.indexOf("Ghost") >= 0) {
                ghostHits++;
            }
        }
        private function onTick(e:Event):void {
            frame++;
            if (frame == 100) {
                trace("M12 dbg " + NovaGame.debugInfo());
            }
            if (frame == 240) {
                var rectOk:Boolean = seen["Box<>PadA"] == true;
                var circOk:Boolean = seen["Disc<>PadB"] == true || seen["PadB<>Disc"] == true;
                var polyOk:Boolean = seen["Wedge<>PadC"] == true || seen["PadC<>Wedge"] == true;
                trace("M12 rect=" + rectOk + " circle=" + circOk + " poly=" + polyOk
                    + " ghost=" + (ghostHits == 0) + " enters=" + enters);
                // Camera follows 'Box' (world y ~= 270): with the camera the
                // display y must sit near stage centre (180), not at 270.
                var box = getChildByName("Box");
                var camOk:Boolean = box != null && box.y > 140 && box.y < 220;
                trace("M12 cam boxY=" + (box == null ? -1 : box.y) + " ok=" + camOk);
                trace("M12DONE");
            }
        }
    }
}
'''

collision_scene = scene(
    "Colliders",
    [
        rect("PadA", 130, 300, 170, 20, (90, 120, 90),
             {"collision": collision(w=170, h=20), "physics": physics("static")}),
        rect("PadB", 320, 300, 170, 20, (90, 120, 90),
             {"collision": collision(w=170, h=20), "physics": physics("static")}),
        rect("PadC", 510, 300, 170, 20, (90, 120, 90),
             {"collision": collision(w=170, h=20), "physics": physics("static")}),
        rect("Box", 140, 40, 40, 40, (255, 210, 74),
             {"collision": collision(w=40, h=40), "physics": physics("dynamic", bounce=0.2)}),
        ellipse("Disc", 320, 40, 36, 36, (120, 200, 255),
                {"collision": collision(shape="circle", w=36, h=36),
                 "physics": physics("dynamic", bounce=0.2)}),
        # Visual is a plain rect; the COLLIDER is the convex triangle
        # (-20,-20) (20,-20) (0,20) relative to the object origin.
        {
            "id": uid(),
            "name": "Wedge",
            "kind": {"kind": "ellipse", "width": 40.0, "height": 40.0},
            "transform": {"x": 500.0, "y": 40.0, "scale_x": 1.0, "scale_y": 1.0, "rotation": 0.0},
            "fill": {"r": 200, "g": 120, "b": 255, "a": 255},
            "game": {"collision": {"shape": {"polygon": {"points": [[-20, -20], [20, -20], [0, 20]]}}, "width": 40.0, "height": 40.0,
                                   "layer": 1, "mask": 4294967295},
                     "physics": physics("dynamic", bounce=0.2)},
        },
        # Layer/mask isolation: layer 2, mask 2 -> interacts with nobody
        # whose mask lacks bit 2 (all other bodies use mask=all, layer=1,
        # so 1 & 2 == 0 -> no interaction either direction).
        rect("Ghost", 140, 250, 40, 40, (255, 80, 80),
             {"collision": collision(w=40, h=40, layer=2, mask=2),
              "physics": physics("static")}),
    ],
    nframes=2,
    game={
        "camera": {"enabled": True, "left": 0.0, "top": 0.0, "right": 1200.0,
                   "bottom": 800.0, "zoom": 1.0, "followSmoothing": 0.05,
                   "deadZone": [0.0, 0.0], "shake": 0.0, "follow": "Box"},
        "world": {"width": 1200.0, "height": 800.0, "spawn": [320.0, 40.0]},
    },
    w=640.0, h=360.0,
)

write_demo(
    "m12-collision-demo.nova",
    project([collision_scene], [script("scripts/Main.as", MAIN_COLLISION)] + gamekit_scripts()),
)

# ---------------------------------------------------------------------------
# Demo 4: particle runtime
# ---------------------------------------------------------------------------

MAIN_PARTICLES = r'''package {
    import flash.display.MovieClip;
    import flash.events.Event;
    import game.NovaGame;

    /// M12 particle demo. Two emitters (burst + continuous, local and world
    /// space) run through the NovaGame bridge; particleCount() is the runtime
    /// probe consumed by the harness.
    public dynamic class Main extends MovieClip {
        private var frame:int = 0;

        public function Main() {
            NovaGame.init(this);
            addEventListener(Event.ENTER_FRAME, onTick);
        }
        private function onTick(e:Event):void {
            frame++;
            if (frame == 30) {
                trace("M12 p30=" + NovaGame.particleCount()
                    + " ok30=" + (NovaGame.particleCount() > 0));
            }
            if (frame == 90) {
                trace("M12 p90=" + NovaGame.particleCount());
            }
            if (frame == 150) {
                trace("M12 p150=" + NovaGame.particleCount()
                    + " ok=" + (NovaGame.particleCount() > 0));
                trace("M12DONE");
            }
        }
    }
}
'''

def emitter_obj(name, x, y, w, h, color, particles):
    o = rect(name, x, y, w, h, color)
    o["particles"] = particles
    return o


def particle_spec(**kw):
    base = {
        "preset": "custom",
        "rate": 0.0,
        "burst": 0,
        "lifetime": [0.6, 1.2],
        "sizeStart": [4.0, 10.0],
        "sizeEnd": [0.0, 2.0],
        "opacityStart": [1.0, 1.0],
        "opacityEnd": [0.0, 0.0],
        "colorStart": {"r": 255, "g": 200, "b": 80, "a": 255},
        "colorEnd": {"r": 255, "g": 80, "b": 40, "a": 0},
        "speed": [60.0, 140.0],
        "direction": -90.0,
        "spread": 40.0,
        "gravity": 300.0,
        "drag": 0.5,
        "rotationSpeed": [-90.0, 90.0],
        "maxParticles": 256,
        "space": "local",
    }
    base.update(kw)
    return base


particles_scene = scene(
    "Particles",
    [
        emitter_obj("Burst", 160, 120, 20, 20, (200, 120, 255),
                    particle_spec(burst=40, rate=0.0, direction=-90.0, spread=60.0,
                                  speed=[80.0, 200.0], lifetime=[0.8, 1.6],
                                  gravity=400.0, space="world")),
        emitter_obj("Fountain", 480, 260, 20, 20, (80, 200, 255),
                    particle_spec(rate=30.0, burst=0, direction=-90.0, spread=25.0,
                                  speed=[120.0, 220.0], lifetime=[0.5, 1.0],
                                  gravity=500.0, space="local")),
        rect("Floor", 320, 345, 640, 30, (80, 90, 110)),
    ],
    nframes=2,
    bg=(20, 24, 34),
)

write_demo(
    "m12-particle-runtime.nova",
    project([particles_scene], [script("scripts/Main.as", MAIN_PARTICLES)] + gamekit_scripts()),
)

print("done")
