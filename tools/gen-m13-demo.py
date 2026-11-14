#!/usr/bin/env python3
"""Generate the M13 runtime-features demo project.

Covers the M13 additions to the game.NovaGame AS3 bridge:
  - Math.* shim (sqrt/sin/floor/pow/atan2/log)
  - kinematic body drive via setBodyVelocity
  - synthetic key injection via debugSetKey (drives a configured
    scene transition end-to-end)
  - tilemap solid-cell collision export (shape-4 grid specs)
  - per-scene body state save/restore across transitions

Run: python tools/gen-m13-demo.py
"""
import json
import re
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SAMPLES = REPO / "samples"


def uid():
    return str(uuid.uuid4())


def layer(name, objects, nframes):
    frame = {"keyframe": True, "objects": objects}
    frames = [dict(frame) for _ in range(nframes)]
    for f in frames[1:]:
        f["keyframe"] = False
    return {"id": uid(), "name": name, "visible": True, "locked": False,
            "kind": "normal", "frames": frames}


def rect(name, x, y, w, h, color, game=None):
    o = {
        "id": uid(), "name": name,
        "kind": {"kind": "rect", "width": w, "height": h, "corner_radius": 0.0},
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
    return {"body": body, "gravityScale": gravity, "friction": friction,
            "bounce": bounce, "controlled": controlled}


def scene(name, objects, nframes=1, game=None, w=640.0, h=360.0, bg=(18, 20, 26)):
    return {
        "id": uid(), "name": name, "width": w, "height": h,
        "background": {"r": bg[0], "g": bg[1], "b": bg[2], "a": 255},
        "fps": 30.0,
        "layers": [layer("Ebene 1", objects, nframes)],
        "game": game,
    }


def script(path, source):
    return {"id": uid(), "name": Path(path).name, "path": path,
            "source": source, "provenance": "original_source"}


def gamekit_scripts():
    src = (REPO / "crates" / "nova-core" / "src" / "gamekit.rs").read_text(encoding="utf-8")
    paths = {
        "INPUT": "scripts/game/Input.as",
        "COLLISION": "scripts/game/Collision.as",
        "PHYSICS": "scripts/game/Physics.as",
        "CAMERA": "scripts/game/Camera.as",
        "RANDOM": "scripts/game/Random.as",
        "NOVA_GAME": "scripts/game/NovaGame.as",
        "MATH": "scripts/Math.as",
    }
    out = []
    for m in re.finditer(r'const (\w+): &str = r#"(.*?)"#;', src, re.S):
        const, body = m.group(1), m.group(2)
        if const in paths:
            out.append(script(paths[const], body))
    return out


# ---------------------------------------------------------------------------
# Scene 1: Boot — Math shim, kinematic drive, injected-key transition
# ---------------------------------------------------------------------------

boot_scene = scene(
    "Boot",
    [
        rect("Mover", 100, 120, 40, 20, (80, 160, 255),
             {"collision": collision(w=40, h=20),
              "physics": physics("kinematic", gravity=0.0)}),
        rect("Pad", 300, 300, 120, 20, (90, 90, 90),
             {"collision": collision(w=120, h=20), "physics": physics("static")}),
    ],
    nframes=2,
    game={
        "world": {"width": 640.0, "height": 360.0, "spawn": [320.0, 60.0]},
        "transitions": [{"key": "Space", "target": "Level"}],
    },
)

# ---------------------------------------------------------------------------
# Scene 2: Level — tilemap solid cells + dynamic hero + state restore
# ---------------------------------------------------------------------------

TILESET_ID = str(uuid.uuid4())
ASSET_ID = str(uuid.uuid4())

# 20x8 grid, 16px cells. Rows 5..7 solid across (ground), plus a pillar at
# columns 4-5 rows 2-4. Cell value 5 -> tileset index 4 -> solid.
COLS, ROWS = 20, 8
cells = [0] * (COLS * ROWS)
for r in range(ROWS):
    for c in range(COLS):
        if r >= 5:
            cells[r * COLS + c] = 5
        elif 4 <= c <= 5 and 2 <= r <= 4:
            cells[r * COLS + c] = 5

tilemap_obj = {
    "id": uid(),
    "name": "Terrain",
    "kind": {
        "kind": "tilemap",
        "tileset": TILESET_ID,
        "tileWidth": 16.0,
        "tileHeight": 16.0,
        "columns": COLS,
        "rows": ROWS,
        "layers": [{"name": "Terrain", "visible": True, "locked": False,
                    "cells": cells}],
    },
    "transform": {"x": 320.0, "y": 300.0, "scale_x": 1.0, "scale_y": 1.0,
                  "rotation": 0.0},
    "fill": {"r": 255, "g": 255, "b": 255, "a": 255},
}

level_scene = scene(
    "Level",
    [
        tilemap_obj,
        rect("Hero", 300, 60, 32, 32, (255, 200, 60),
             {"collision": collision(w=32, h=32),
              "physics": physics("dynamic", gravity=1.0, bounce=0.0)}),
    ],
    nframes=2,
    game={
        "world": {"width": 640.0, "height": 360.0, "spawn": [320.0, 60.0]},
        "transitions": [{"key": "KeyR", "target": "Boot"}],
    },
)

# ---------------------------------------------------------------------------
# Main.as — drives every M13 feature and traces verifiable markers
# ---------------------------------------------------------------------------

MAIN_M13 = r'''package {
    import flash.display.MovieClip;
    import flash.events.Event;
    import game.NovaGame;

    /// M13 runtime-features demo: Math shim, kinematic drive, injected-key
    /// transition, tilemap collision and per-scene state restore.
    public dynamic class Main extends MovieClip {
        private var frame:int = 0;
        private var heroY1:Number = -1;
        private var tileEnter:Boolean = false;

        public function Main() {
            NovaGame.init(this);
            addEventListener(Event.ENTER_FRAME, onTick);
            addEventListener("nova:collisionEnter", onCollide);
        }

        private function onCollide(e:Event):void {
            var info:Object = NovaGame.eventInfo;
            if (info != null && info.a == "Hero" && info.b == "Terrain") {
                tileEnter = true;
            }
        }

        private function onTick(e:Event):void {
            frame++;
            // --- Math shim -------------------------------------------------
            if (frame == 5) {
                var sqrtOk:Boolean = Math.abs(Math.sqrt(2) - 1.41421356) < 0.001;
                var sinOk:Boolean = Math.abs(Math.sin(1) - 0.84147098) < 0.001;
                var floorOk:Boolean = Math.floor(-1.5) == -2;
                var powOk:Boolean = Math.pow(2, 10) == 1024;
                var atanOk:Boolean = Math.abs(Math.atan2(1, 1) - 0.78539816) < 0.001;
                var logOk:Boolean = Math.abs(Math.log(Math.E) - 1) < 0.001;
                trace("M13 math sqrt=" + Math.sqrt(2) + " sin=" + Math.sin(1)
                    + " floor=" + Math.floor(-1.5) + " pow=" + Math.pow(2, 10)
                    + " atan2=" + Math.atan2(1, 1) + " logE=" + Math.log(Math.E));
                trace("M13 math ok=" + (sqrtOk && sinOk && floorOk && powOk && atanOk && logOk));
            }
            // --- kinematic drive --------------------------------------------
            if (frame == 15) {
                var ok:Boolean = NovaGame.setBodyVelocity("Mover", 90, 0);
                trace("M13 kin set=" + ok);
            }
            if (frame == 45) {
                var m = getChildByName("Mover");
                // ~30 frames at 90 px/s -> x ~= 190 (spawn 100).
                var moved:Boolean = m != null && m.x > 160 && m.x < 230;
                trace("M13 kin x=" + (m == null ? -1 : m.x) + " ok=" + moved);
                trace("M13 dbg " + NovaGame.debugInfo());
            }
            // --- injected key -> configured transition ----------------------
            if (frame == 60) {
                NovaGame.debugSetKey(32, true);   // Space -> Level
            }
            if (frame == 61) {
                NovaGame.debugSetKey(32, false);
            }
            if (frame == 80) {
                trace("M13 scene=" + NovaGame.currentScene()
                    + " ok=" + (NovaGame.currentScene() == "Level"));
            }
            // --- tilemap collision ------------------------------------------
            if (frame == 140) {
                var h = getChildByName("Hero");
                heroY1 = h == null ? -1 : h.y;
                trace("M13 tile enter=" + tileEnter + " heroY=" + heroY1);
                trace("M13 dbg2 " + NovaGame.debugInfo());
            }
            // --- state restore: leave and re-enter Level --------------------
            if (frame == 160) {
                NovaGame.gotoScene("Boot");
            }
            if (frame == 170) {
                NovaGame.gotoScene("Level");
            }
            if (frame == 210) {
                var h2 = getChildByName("Hero");
                var y2:Number = h2 == null ? -1 : h2.y;
                // Restored snapshot keeps the landed position (~heroY1),
                // not the authored spawn (60).
                var restored:Boolean = heroY1 > 0 && y2 > 0
                    && Math.abs(y2 - heroY1) < 40 && y2 > 150;
                trace("M13 state y1=" + heroY1 + " y2=" + y2 + " ok=" + restored);
            }
            if (frame == 230) {
                NovaGame.gotoScene("Boot");
            }
            if (frame == 250) {
                trace("M13 scene2=" + NovaGame.currentScene()
                    + " ok=" + (NovaGame.currentScene() == "Boot"));
                trace("M13DONE");
            }
        }
    }
}
'''

# ---------------------------------------------------------------------------
# Project assembly — reuse the M11 tileset/asset so solid flags are real
# ---------------------------------------------------------------------------

m11 = json.loads((SAMPLES / "m11-tilemap-platformer.nova").read_text(encoding="utf-8"))
asset = dict(m11["project"]["assets"][0])
asset["id"] = ASSET_ID
tileset = dict(m11["project"]["tilesets"][0])
tileset["id"] = TILESET_ID
tileset["asset"] = ASSET_ID
# Only tile index 4 (cell value 5) is solid.
tileset["solid"] = [i == 4 for i in range(len(tileset.get("solid", [])) or 16)]
if len(tileset["solid"]) < 16:
    tileset["solid"] = (tileset["solid"] + [False] * 16)[:16]
tileset["solid"][4] = True

project = {
    "id": uid(),
    "metadata": {"name": "M13 Runtime Features", "author": "",
                 "description": "Mega Milestone 13 runtime parity demo",
                 "createdUnix": 1763000000, "modifiedUnix": 1763000000},
    "scenes": [boot_scene, level_scene],
    "symbols": [],
    "assets": [asset],
    "tilesets": [tileset],
    "scripts": [script("scripts/Main.as", MAIN_M13)] + gamekit_scripts(),
    "export": {"target": "avm2", "swfVersion": 10, "compression": "zlib",
               "includeScripts": True, "exportAllScenes": False},
    "activeScene": boot_scene["id"],
}
envelope = {
    "format": "nova-flash-studio",
    "formatVersion": 4,
    "project": project,
}
out = SAMPLES / "m13-runtime-features.nova"
out.write_text(json.dumps(envelope, indent=1), encoding="utf-8")
print("wrote", out)
