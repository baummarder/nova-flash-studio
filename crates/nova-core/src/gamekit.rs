//! Built-in ActionScript 3 game kit shipped with game projects.
//!
//! Each entry is `(virtual path, source)` and becomes a regular project
//! [`Script`]. The sources deliberately use only the subset `nova-as`
//! compiles: no interfaces, no `for-in`, no closures, no `Array`/`Vector`
//! annotations and no `Math` (a tiny LCG lives in `game.Random`).
//!
//! These helpers target the Ruffle/AVM2 runtime; whether every member works
//! there is verified per game sample, not assumed.

use crate::model::{Project, Script};

/// `(path, source)` pairs added by [`install`].
pub const FILES: &[(&str, &str)] = &[
    ("scripts/game/Input.as", INPUT),
    ("scripts/game/Collision.as", COLLISION),
    ("scripts/game/Physics.as", PHYSICS),
    ("scripts/game/Camera.as", CAMERA),
    ("scripts/game/Random.as", RANDOM),
    ("scripts/game/NovaGame.as", NOVA_GAME),
];

const INPUT: &str = r#"package game {
    import flash.display.Stage;
    import flash.events.KeyboardEvent;
    import flash.events.MouseEvent;

    /// Keyboard/mouse state helper. Call `init(stage)` once from the document
    /// class constructor, `poll()` once per frame, then query `down`/`pressed`.
    public class Input {
        private static var held:Object = {};
        private static var consumed:Object = {};
        private static var target:Stage = null;
        public static var mouseX:Number = 0;
        public static var mouseY:Number = 0;

        public static function init(stage:Stage):void {
            target = stage;
            stage.addEventListener(KeyboardEvent.KEY_DOWN, onKeyDown);
            stage.addEventListener(KeyboardEvent.KEY_UP, onKeyUp);
            stage.addEventListener(MouseEvent.MOUSE_MOVE, onMouseMove);
        }

        private static function onKeyDown(e:KeyboardEvent):void {
            held[e.keyCode] = true;
        }

        private static function onKeyUp(e:KeyboardEvent):void {
            held[e.keyCode] = false;
            consumed[e.keyCode] = false;
        }

        private static function onMouseMove(e:MouseEvent):void {
            mouseX = e.stageX;
            mouseY = e.stageY;
        }

        /// Called once per frame after all `pressed` queries of the previous
        /// frame are done. Resets edge-detection memory for released keys.
        public static function poll():void {
            // Nothing to copy: `consumed` is cleared lazily inside `pressed`.
        }

        public static function down(code:int):Boolean {
            return held[code] == true;
        }

        /// True only on the frame the key went down (edge trigger).
        public static function pressed(code:int):Boolean {
            if (held[code] == true && consumed[code] != true) {
                consumed[code] = true;
                return true;
            }
            if (held[code] != true) {
                consumed[code] = false;
            }
            return false;
        }
    }
}
"#;

const COLLISION: &str = r#"package game {
    import flash.display.DisplayObject;

    /// Axis-aligned and radial collision helpers operating on plain numbers.
    /// Display objects are measured by their `x`/`y`/`width`/`height` bounds,
    /// which works for programmatically drawn sprites.
    public class Collision {
        public static function aabb(
            x1:Number, y1:Number, w1:Number, h1:Number,
            x2:Number, y2:Number, w2:Number, h2:Number
        ):Boolean {
            return x1 < x2 + w2 && x1 + w1 > x2 && y1 < y2 + h2 && y1 + h1 > y2;
        }

        public static function circle(
            cx1:Number, cy1:Number, r1:Number,
            cx2:Number, cy2:Number, r2:Number
        ):Boolean {
            var dx:Number = cx1 - cx2;
            var dy:Number = cy1 - cy2;
            var rr:Number = r1 + r2;
            return dx * dx + dy * dy < rr * rr;
        }

        public static function pointInRect(
            px:Number, py:Number, x:Number, y:Number, w:Number, h:Number
        ):Boolean {
            return px >= x && px <= x + w && py >= y && py <= y + h;
        }

        /// AABB test between two display objects using their bounds.
        public static function objectsOverlap(a:DisplayObject, b:DisplayObject):Boolean {
            return aabb(a.x, a.y, a.width, a.height, b.x, b.y, b.width, b.height);
        }
    }
}
"#;

const PHYSICS: &str = r#"package game {
    import flash.display.DisplayObject;

    /// Minimal velocity/gravity integration for display objects. Velocity is
    /// stored in dynamic `vx`/`vy` properties on the object itself.
    public class Physics {
        /// Integrates one body: gravity, then position. `gravity` is px/s^2.
        public static function stepBody(body:DisplayObject, dt:Number, gravity:Number):void {
            body.vy = body.vy + gravity * dt;
            body.x = body.x + body.vx * dt;
            body.y = body.y + body.vy * dt;
        }

        /// Applies a horizontal acceleration, clamped to `maxSpeed`.
        public static function accelerate(body:DisplayObject, accel:Number, maxSpeed:Number, dt:Number):void {
            body.vx = body.vx + accel * dt;
            if (body.vx > maxSpeed) {
                body.vx = maxSpeed;
            }
            if (body.vx < -maxSpeed) {
                body.vx = -maxSpeed;
            }
        }

        /// Simple ground collision response: keeps `body` above `floorY`
        /// (bottom edge) and kills downward velocity on contact.
        /// Returns true while the body rests on the floor.
        public static function floor(body:DisplayObject, floorY:Number):Boolean {
            if (body.y + body.height > floorY) {
                body.y = floorY - body.height;
                if (body.vy > 0) {
                    body.vy = 0;
                }
                return true;
            }
            return false;
        }
    }
}
"#;

const CAMERA: &str = r#"package game {
    import flash.display.DisplayObject;

    /// Follow camera for games built on a single world container. Apply the
    /// result with `applyTo(world)` each frame.
    public class Camera {
        public var x:Number = 0;
        public var y:Number = 0;
        public var zoom:Number = 1;
        private var target:DisplayObject = null;
        private var lerp:Number = 6;

        public function follow(mc:DisplayObject):void {
            target = mc;
        }

        /// Advances the camera toward the follow target. Pass world bounds as
        /// four numbers; use 0/0/0/0 for unbounded.
        public function update(
            dt:Number, viewW:Number, viewH:Number,
            boundL:Number, boundT:Number, boundR:Number, boundB:Number
        ):void {
            if (target == null) {
                return;
            }
            var tx:Number = target.x + target.width / 2;
            var ty:Number = target.y + target.height / 2;
            var t:Number = dt * lerp;
            if (t > 1) {
                t = 1;
            }
            x = x + (tx - x) * t;
            y = y + (ty - y) * t;
            var halfW:Number = viewW / (2 * zoom);
            var halfH:Number = viewH / (2 * zoom);
            if (boundR > boundL) {
                var loX:Number = boundL + halfW;
                var hiX:Number = boundR - halfW;
                if (loX <= hiX) {
                    if (x < loX) { x = loX; }
                    if (x > hiX) { x = hiX; }
                }
            }
            if (boundB > boundT) {
                var loY:Number = boundT + halfH;
                var hiY:Number = boundB - halfH;
                if (loY <= hiY) {
                    if (y < loY) { y = loY; }
                    if (y > hiY) { y = hiY; }
                }
            }
        }

        /// Positions `world` so the camera centre lands mid-view.
        public function applyTo(world:DisplayObject):void {
            world.x = world.stage.stageWidth / 2 - x * zoom;
            world.y = world.stage.stageHeight / 2 - y * zoom;
            world.scaleX = zoom;
            world.scaleY = zoom;
        }
    }
}
"#;

const RANDOM: &str = r#"package game {
    import flash.utils.getTimer;

    /// Small deterministic PRNG (linear congruential generator). Seeded from
    /// `getTimer()`; call `srand` for a fixed sequence in tests.
    public class Random {
        private static var state:Number = getTimer() + 1;

        public static function srand(seed:Number):void {
            state = seed;
        }

        /// Returns a value in [0, 1).
        public static function next():Number {
            state = (state * 1103515245 + 12345) % 2147483648;
            return state / 2147483648;
        }

        /// Returns a value in [0, bound).
        public static function range(bound:Number):Number {
            return next() * bound;
        }
    }
}
"#;

const NOVA_GAME: &str = r#"package game {
    import flash.display.MovieClip;
    import flash.display.DisplayObject;
    import flash.events.Event;
    import flash.events.KeyboardEvent;
    import flash.utils.getTimer;
    import game.Input;
    import game.NovaGameData;

    /// NØVA game runtime bridge for exported SWFs.
    ///
    /// Mirrors the editor preview runtime (`nova_core::game` /
    /// `GameRuntime`) inside AVM2: scene lifecycle driven by the concatenated
    /// scene timeline, key-triggered scene transitions, a typed event bus on
    /// custom `nova:*` event types, rect/circle/polygon collision with
    /// layer/mask filtering and enter/exit events, and per-scene camera,
    /// world and input configuration read from the generated
    /// `game.NovaGameData` table.
    ///
    /// Usage from the document class:
    ///   public function Main() {
    ///       NovaGame.init(this);
    ///   }
    ///
    /// Listen for events on the document clip itself:
    ///   addEventListener("nova:collisionEnter", onHit);
    ///   // NovaGame.eventInfo.a / .b carry the body names.
    ///
    /// Deliberate subset constraints: no Math (own helpers below), Object
    /// hash maps instead of Array, static methods only.
    public class NovaGame {
        // ------------------------------------------------------------------
        // Public state
        // ------------------------------------------------------------------
        public static var time:Number = 0;
        public static var paused:Boolean = false;
        public static var sceneName:String = "";
        public static var eventInfo:Object = {};
        public static var camX:Number = 0;
        public static var camY:Number = 0;
        public static var camZoom:Number = 1;

        // ------------------------------------------------------------------
        // Internals
        // ------------------------------------------------------------------
        private static var root:MovieClip = null;
        private static var started:Boolean = false;
        private static var sceneIdx:int = -1;
        private static var sceneStart:int = 1;
        private static var sceneEnd:int = 1;
        private static var sceneTick:int = 0;
        private static var prevKeys:Object = {};
        private static var bodies:Object = {};
        private static var bodyCount:int = 0;
        private static var contacts:Object = {};
        private static var contactKeys:Object = {};
        private static var contactCount:int = 0;
        private static var bodiesReady:Boolean = false;
        private static var camCfg:Object = null;
        private static var worldW:Number = 0;
        private static var worldH:Number = 0;
        private static var inputCfg:Object = null;
        private static var camBase:Object = null;
        private static var viewW:Number = 550;
        private static var viewH:Number = 400;
        private static var timeSec:Number = 0;
        private static var dispatchDepth:int = 0;
        private static var droppedEvents:int = 0;

        // Physics constants mirrored from nova_core::game.
        private static const GRAVITY:Number = 2200.0;
        private static const MOVE_ACCEL:Number = 2600.0;
        private static const JUMP_VELOCITY:Number = -560.0;
        private static const MAX_SPEED:Number = 320.0;
        private static const GROUND_FRICTION:Number = 10.0;
        private static const CAMERA_LERP:Number = 6.0;
        private static const MAX_DT:Number = 0.1;
        private static const MAX_DISPATCH:Number = 8;

        // ------------------------------------------------------------------
        // Lifecycle
        // ------------------------------------------------------------------

        /// Attach the runtime to the document clip. Call once from the
        /// document class constructor.
        public static function init(mc:MovieClip):void {
            if (root != null) {
                return;
            }
            root = mc;
            NovaGameData.init();
            if (mc.stage != null) {
                Input.init(mc.stage);
                viewW = mc.stage.stageWidth;
                viewH = mc.stage.stageHeight;
                mc.stage.addEventListener(KeyboardEvent.KEY_DOWN, onKeyDown);
                mc.stage.addEventListener(KeyboardEvent.KEY_UP, onKeyUp);
            }
            mc.addEventListener(Event.ENTER_FRAME, tick);
            sceneIdx = sceneForFrame(mc.currentFrame);
            if (sceneIdx >= 0) {
                enterScene(sceneIdx, false);
            }
            started = true;
            emit("nova:gameStart", null);
        }

        public static function start():void {
            if (!paused) {
                return;
            }
            paused = false;
            emit("nova:gameResume", null);
        }

        public static function pause():void {
            if (paused || !started) {
                return;
            }
            paused = true;
            emit("nova:gamePause", null);
        }

        public static function resume():void {
            start();
        }

        /// Reloads the current scene frame and resets simulation state.
        public static function restart():void {
            if (root == null) {
                return;
            }
            var idx:int = sceneIdx;
            if (idx < 0) {
                idx = sceneForFrame(root.currentFrame);
            }
            bodies = {};
            bodyCount = 0;
            bodiesReady = false;
            contacts = {};
            contactKeys = {};
            contactCount = 0;
            clearEmitters();
            syncEmitters();
            time = 0;
            timeSec = 0;
            if (idx >= 0) {
                enterScene(idx, true);
            }
            paused = false;
            emit("nova:gameRestart", null);
        }

        public static function stop():void {
            if (!started) {
                return;
            }
            started = false;
            paused = false;
            time = 0;
            emit("nova:gameStop", null);
        }

        // ------------------------------------------------------------------
        // Queries
        // ------------------------------------------------------------------

        public static function getTime():Number {
            return timeSec;
        }

        public static function isPaused():Boolean {
            return paused;
        }

        public static function currentScene():String {
            return sceneName;
        }

        public static function sceneCount():int {
            return NovaGameData.SCENE_COUNT;
        }

        /// Switch to the scene with the given name. Returns false and emits a
        /// warning event when the target does not exist.
        public static function gotoScene(name:String):Boolean {
            var idx:int = sceneIndexByName(name);
            if (idx < 0) {
                trace("NovaGame: unbekanntes Szenenziel '" + name + "'");
                eventInfo = { kind: "error", scene: sceneName, a: sceneName, b: name, key: 0, data: null };
                emit("nova:sceneError", name);
                return false;
            }
            if (idx == sceneIdx) {
                return true;
            }
            enterScene(idx, true);
            return true;
        }

        /// Dispatch a custom event of type `type` on the document clip.
        /// `data` is exposed to listeners via `NovaGame.eventInfo.data`.
        public static function emit(type:String, data):void {
            if (root == null) {
                return;
            }
            if (dispatchDepth >= MAX_DISPATCH) {
                droppedEvents++;
                return;
            }
            eventInfo.data = data;
            dispatchDepth++;
            root.dispatchEvent(new Event(type));
            dispatchDepth--;
        }

        public static function droppedEventCount():int {
            return droppedEvents;
        }

        /// Live particle count across all emitters of the current scene.
        /// Useful for runtime validation.
        public static function particleCount():int {
            var n:int = 0;
            for (var i:int = 0; i < emitCount; i++) {
                var e:Object = emitters[i];
                if (e != null) {
                    n += e.live;
                }
            }
            return n;
        }

        /// Teleports a named dynamic/kinematic body and zeroes its velocity.
        /// Mainly a test/debug hook; returns false for unknown names.
        public static function setBodyPosition(name:String, x:Number, y:Number):Boolean {
            for (var i:int = 0; i < bodyCount; i++) {
                var b:Object = bodies[i];
                if (b != null && b.name == name) {
                    b.px = x;
                    b.py = y;
                    b.vx = 0;
                    b.vy = 0;
                    if (b.o != null) {
                        b.o.x = x;
                        b.o.y = y;
                    }
                    invalidatePoints(b);
                    return true;
                }
            }
            return false;
        }

        /// Diagnostic dump for runtime validation: scene, body count and the
        /// first few body positions/types.
        public static function debugInfo():String {
            var s:String = "scene=" + sceneName + " bodies=" + bodyCount
                + " contacts=" + contactCount;
            for (var i:int = 0; i < bodyCount && i < 6; i++) {
                var b:Object = bodies[i];
                s += " | " + b.name + "(" + b.btype + "," + b.shape + ","
                    + b.px + "," + b.py + "," + b.extentW + "x" + b.extentH + ")";
            }
            return s;
        }

        // ------------------------------------------------------------------
        // Frame loop
        // ------------------------------------------------------------------

        private static function tick(e:Event):void {
            if (root == null || paused || !started) {
                return;
            }
            // Fixed-step simulation: one ENTER_FRAME == one authored frame,
            // independent of the renderer's actual frame rate. This matches
            // Flash-style frame semantics and keeps runtime behaviour
            // deterministic for tests (Ruffle may tick faster than the SWF
            // fps when uncapped).
            var dt:Number = 1.0 / NovaGameData.FPS;
            if (dt > MAX_DT) {
                dt = MAX_DT;
            }
            time += dt;
            timeSec += dt;

            // NovaGame owns the playhead inside the active scene range: the
            // concatenated SWF timeline would otherwise play straight through
            // neighbouring scenes. Each ENTER_FRAME advances one frame inside
            // [sceneStart, sceneEnd] and loops; gotoAndStop keeps the position
            // deterministic (no reliance on currentFrame updating synchronously).
            if (sceneIdx >= 0) {
                sceneTick++;
                var len:int = sceneEnd - sceneStart + 1;
                if (len < 1) {
                    len = 1;
                }
                var target:int = sceneStart + (sceneTick % len);
                if (root.currentFrame != target) {
                    root.gotoAndStop(target);
                }
            } else {
                var idx:int = sceneForFrame(root.currentFrame);
                if (idx >= 0) {
                    enterScene(idx, true);
                }
            }

            checkTransitions();
            syncBodies();
            stepPhysics(dt);
            stepCamera(dt);
            stepParticles(dt);
            Input.poll();
        }

        private static function sceneForFrame(frame:int):int {
            for (var i:int = 0; i < NovaGameData.SCENE_COUNT; i++) {
                if (frame >= NovaGameData.SCENE_START[i] && frame <= NovaGameData.SCENE_END[i]) {
                    return i;
                }
            }
            return -1;
        }

        private static function sceneIndexByName(name:String):int {
            for (var i:int = 0; i < NovaGameData.SCENE_COUNT; i++) {
                if (NovaGameData.SCENE_NAMES[i] == name) {
                    return i;
                }
            }
            return -1;
        }

        private static function enterScene(idx:int, announce:Boolean):void {
            var oldName:String = sceneName;
            sceneIdx = idx;
            sceneName = NovaGameData.SCENE_NAMES[idx];
            sceneStart = NovaGameData.SCENE_START[idx];
            sceneEnd = NovaGameData.SCENE_END[idx];
            sceneTick = 0;
            if (root != null) {
                root.gotoAndStop(sceneStart);
            }
            bodies = {};
            bodyCount = 0;
            bodiesReady = false;
            contacts = {};
            contactKeys = {};
            contactCount = 0;
            prevKeys = {};
            camBase = {};
            clearEmitters();
            syncEmitters();
            loadSceneConfig(idx);
            if (announce) {
                if (oldName != "") {
                    eventInfo = { kind: "unload", scene: oldName, a: oldName, b: sceneName, key: 0, data: null };
                    emit("nova:sceneUnload", oldName);
                }
                eventInfo = { kind: "transition", scene: sceneName, a: oldName, b: sceneName, key: 0, data: null };
                emit("nova:sceneTransition", oldName + "->" + sceneName);
                eventInfo = { kind: "load", scene: sceneName, a: oldName, b: sceneName, key: 0, data: null };
                emit("nova:sceneLoad", sceneName);
            } else {
                eventInfo = { kind: "load", scene: sceneName, a: "", b: sceneName, key: 0, data: null };
                emit("nova:sceneLoad", sceneName);
            }
        }

        private static function loadSceneConfig(idx:int):void {
            var name:String = NovaGameData.SCENE_NAMES[idx];
            var cam:String = NovaGameData.CAMERAS[name];
            camCfg = null;
            if (cam != null && cam != "") {
                camCfg = splitPipe(cam);
                camZoom = f(camCfg, 5);
                camX = (worldWFor(idx)) / 2;
                camY = (worldHFor(idx)) / 2;
            } else {
                camZoom = 1;
                camX = viewW / 2;
                camY = viewH / 2;
            }
            var world:String = NovaGameData.WORLDS[name];
            if (world != null && world != "") {
                var w:Object = splitPipe(world);
                worldW = f(w, 0);
                worldH = f(w, 1);
            } else {
                worldW = viewW;
                worldH = viewH;
            }
            var inp:String = NovaGameData.INPUTS[name];
            inputCfg = null;
            if (inp != null && inp != "") {
                inputCfg = splitPipe(inp);
            }
        }

        private static function worldWFor(idx:int):Number {
            var name:String = NovaGameData.SCENE_NAMES[idx];
            var world:String = NovaGameData.WORLDS[name];
            if (world != null && world != "") {
                var w:Object = splitPipe(world);
                if (f(w, 0) > 0) {
                    return f(w, 0);
                }
            }
            return viewW;
        }

        private static function worldHFor(idx:int):Number {
            var name:String = NovaGameData.SCENE_NAMES[idx];
            var world:String = NovaGameData.WORLDS[name];
            if (world != null && world != "") {
                var w:Object = splitPipe(world);
                if (f(w, 1) > 0) {
                    return f(w, 1);
                }
            }
            return viewH;
        }

        // ------------------------------------------------------------------
        // Input
        // ------------------------------------------------------------------

        private static function onKeyDown(e:KeyboardEvent):void {
            eventInfo = { kind: "key", scene: sceneName, a: "", b: "", key: e.keyCode, data: null };
            emit("nova:keyPressed", e.keyCode);
        }

        private static function onKeyUp(e:KeyboardEvent):void {
            eventInfo = { kind: "key", scene: sceneName, a: "", b: "", key: e.keyCode, data: null };
            emit("nova:keyReleased", e.keyCode);
        }

        private static function inputDown(slot:int):Boolean {
            if (inputCfg == null || slot < 0 || slot >= inputCfg.n) {
                return false;
            }
            return Input.down(parseInt(inputCfg[slot], 10));
        }

        private static function checkTransitions():void {
            for (var i:int = 0; i < NovaGameData.TRANS_COUNT; i++) {
                var spec:String = NovaGameData.TRANSITIONS[i];
                var p:Object = splitPipe(spec);
                if (p.n < 2) {
                    continue;
                }
                var code:int = parseInt(p[0], 10);
                var down:Boolean = Input.down(code);
                var was:Boolean = prevKeys[code] == true;
                prevKeys[code] = down;
                if (down && !was) {
                    gotoScene(p[1]);
                    return;
                }
            }
        }

        // ------------------------------------------------------------------
        // Bodies / collision
        // ------------------------------------------------------------------

        /// (Re)builds body records for the current scene from NovaGameData.
        private static function syncBodies():void {
            if (bodiesReady || sceneName == "") {
                return;
            }
            bodiesReady = true;
            var table:Object = NovaGameData.COLLIDERS[sceneName];
            if (table == null) {
                return;
            }
            var count:int = table.count;
            for (var i:int = 0; i < count; i++) {
                var spec:String = table[i];
                var p:Object = splitPipe(spec);
                if (p.n < 12) {
                    continue;
                }
                var objName:String = p[0];
                var obj:DisplayObject = root.getChildByName(objName);
                if (obj == null) {
                    continue;
                }
                var body:Object = {};
                body.name = objName;
                body.o = obj;
                body.shape = parseFloat(p[1]);
                body.w = parseFloat(p[2]);
                body.h = parseFloat(p[3]);
                body.layer = parseFloat(p[4]);
                body.mask = parseFloat(p[5]);
                body.btype = parseFloat(p[6]);
                body.gx = parseFloat(p[7]);
                body.fr = parseFloat(p[8]);
                body.bo = parseFloat(p[9]);
                body.ctrl = parseFloat(p[10]);
                body.npts = 0;
                body.pts = {};
                var pts:String = p[11];
                if (pts != "") {
                    loadPoints(body, pts);
                }
                body.vx = 0;
                body.vy = 0;
                body.grounded = false;
                body.px = obj.x;
                body.py = obj.y;
                bodies[i] = body;
            }
            bodyCount = count;
        }

        private static function loadPoints(body:Object, pts:String):void {
            var parts:Object = splitSemi(pts);
            var n:int = parts.n;
            if (n > 64) {
                n = 64;
            }
            for (var i:int = 0; i < n; i++) {
                var xy:Object = splitComma(parts[i]);
                var pt:Object = {};
                pt[0] = parseFloat(xy[0]);
                pt[1] = parseFloat(xy[1]);
                body.pts[i] = pt;
            }
            body.npts = n;
        }

        /// Integrates forces, moves bodies and resolves collisions — mirrors
        /// `nova_core::game::step` phase order (forces, then resolve, then
        /// world clamp).
        private static function stepPhysics(dt:Number):void {
            if (bodyCount == 0) {
                return;
            }
            // Phase 1: forces + integration.
            for (var i:int = 0; i < bodyCount; i++) {
                var a:Object = bodies[i];
                if (a == null || a.o == null) {
                    continue;
                }
                if (a.btype == 0) {
                    // Static bodies keep their authored position; the bridge
                    // does not adopt display-transform writes (the camera
                    // owns those). Use setBodyPosition() to move them.
                    continue;
                }
                if (a.btype == 2) {
                    // Kinematic: velocity only.
                    a.px += a.vx * dt;
                    a.py += a.vy * dt;
                } else {
                    // Dynamic.
                    a.vy += GRAVITY * a.gx * dt;
                    var damp:Number = 1.0 - a.fr * dt;
                    if (damp < 0) {
                        damp = 0;
                    }
                    a.vx *= damp;
                    a.vy *= damp;
                    if (a.ctrl == 1) {
                        if (inputDown(0)) {
                            a.vx -= MOVE_ACCEL * dt;
                        }
                        if (inputDown(1)) {
                            a.vx += MOVE_ACCEL * dt;
                        }
                        if (a.vx > MAX_SPEED) {
                            a.vx = MAX_SPEED;
                        }
                        if (a.vx < -MAX_SPEED) {
                            a.vx = -MAX_SPEED;
                        }
                        if (inputDown(4) && a.grounded) {
                            a.vy = JUMP_VELOCITY;
                            a.grounded = false;
                        }
                        if (a.grounded) {
                            var gd:Number = 1.0 - GROUND_FRICTION * dt;
                            if (gd < 0) {
                                gd = 0;
                            }
                            a.vx *= gd;
                        }
                    }
                    a.px += a.vx * dt;
                    a.py += a.vy * dt;
                    a.grounded = false;
                }
            }
            // Phase 2a: contact detection on post-integration, PRE-resolution
            // positions. Measuring after resolvePair would report resting
            // contacts as "no overlap" (depth ~0) and lose enter/exit events.
            // Cached world-space polygon vertices are stale after integration,
            // so drop them before measuring.
            for (var ic:int = 0; ic < bodyCount; ic++) {
                var bc:Object = bodies[ic];
                if (bc != null) {
                    invalidatePoints(bc);
                }
            }
            var current:Object = {};
            var currentKeys:Object = {};
            var currentCount:int = 0;
            for (var ci:int = 0; ci < bodyCount; ci++) {
                var ca:Object = bodies[ci];
                if (ca == null) {
                    continue;
                }
                for (var cj:int = ci + 1; cj < bodyCount; cj++) {
                    var cb:Object = bodies[cj];
                    if (cb == null || !interacts(ca, cb)) {
                        continue;
                    }
                    if (collide(ca, cb) != null) {
                        var key:String = pairKey(ca.name, cb.name);
                        if (current[key] != true) {
                            current[key] = true;
                            currentKeys[currentCount] = key;
                            currentCount++;
                        }
                    }
                }
            }
            // Phase 2b: positional correction, dynamic vs non-dynamic only
            // (mirrors the preview's resolution pass).
            for (var i2:int = 0; i2 < bodyCount; i2++) {
                var da:Object = bodies[i2];
                if (da == null || da.btype != 1) {
                    continue;
                }
                for (var j:int = 0; j < bodyCount; j++) {
                    if (j == i2) {
                        continue;
                    }
                    var db:Object = bodies[j];
                    if (db == null || db.btype == 1) {
                        continue;
                    }
                    if (!interacts(da, db)) {
                        continue;
                    }
                    resolvePair(da, db);
                }
            }
            // Phase 3: world bounds + write back.
            for (var k:int = 0; k < bodyCount; k++) {
                var b:Object = bodies[k];
                if (b == null || b.o == null) {
                    continue;
                }
                if (b.btype != 0) {
                    var hw:Number = extentW(b) / 2;
                    var hh:Number = extentH(b) / 2;
                    var minX:Number = hw;
                    var maxX:Number = worldW - hw;
                    if (maxX < minX) {
                        maxX = minX;
                    }
                    var minY:Number = hh;
                    var maxY:Number = worldH - hh;
                    if (maxY < minY) {
                        maxY = minY;
                    }
                    if (b.px < minX) { b.px = minX; }
                    if (b.px > maxX) { b.px = maxX; }
                    if (b.py < minY) { b.py = minY; }
                    if (b.py > maxY) { b.py = maxY; }
                    b.o.x = b.px;
                    b.o.y = b.py;
                }
            }
            updateContacts(current, currentKeys, currentCount);
        }

        private static function extentW(b:Object):Number {
            if (b.w > 0) {
                return b.w;
            }
            return 32;
        }

        private static function extentH(b:Object):Number {
            if (b.shape == 1) {
                return extentW(b);
            }
            if (b.h > 0) {
                return b.h;
            }
            return extentW(b);
        }

        private static function interacts(a:Object, b:Object):Boolean {
            // Shape 3 = physics-only body: it moves but has no collider.
            if (a.shape == 3 || b.shape == 3) {
                return false;
            }
            return (a.layer & b.mask) != 0 && (b.layer & a.mask) != 0;
        }

        /// SAT contact between two bodies; returns Object {nx,ny,depth} or null.
        private static function collide(a:Object, b:Object):Object {
            var asCircle:Boolean = a.shape == 1;
            var bsCircle:Boolean = b.shape == 1;
            if (asCircle && bsCircle) {
                return circleCircle(a, b);
            }
            if (asCircle) {
                return circlePoly(a, b);
            }
            if (bsCircle) {
                var c:Object = circlePoly(b, a);
                if (c == null) {
                    return null;
                }
                // Normal must point from b(=poly a-side?) — swap: contact was
                // computed circle=b vs poly=a, normal points a->b; flip it.
                c.nx = -c.nx;
                c.ny = -c.ny;
                return c;
            }
            return polyPoly(a, b);
        }

        private static function circleCircle(a:Object, b:Object):Object {
            var ra:Number = extentW(a) / 2;
            var rb:Number = extentW(b) / 2;
            var dx:Number = a.px - b.px;
            var dy:Number = a.py - b.py;
            var dist:Number = fSqrt(dx * dx + dy * dy);
            var overlap:Number = ra + rb - dist;
            if (overlap <= 0) {
                return null;
            }
            if (dist < 0.001) {
                return { nx: 0, ny: -1, depth: overlap };
            }
            return { nx: dx / dist, ny: dy / dist, depth: overlap };
        }

        private static function circlePoly(circle:Object, poly:Object):Object {
            var r:Number = extentW(circle) / 2;
            var n:int = polyPoints(poly, null);
            if (n < 3) {
                // Degenerate: fall back to AABB overlap.
                var hw:Number = extentW(poly) / 2;
                var hh:Number = extentH(poly) / 2;
                var ox:Number = fMin(circle.px + r, poly.px + hw) - fMax(circle.px - r, poly.px - hw);
                var oy:Number = fMin(circle.py + r, poly.py + hh) - fMax(circle.py - r, poly.py - hh);
                if (ox > 0 && oy > 0) {
                    return { nx: 0, ny: -1, depth: fMin(ox, oy) };
                }
                return null;
            }
            var pxs:Object = worldPoints(poly, n, "ptsWorld");
            // Closest point on polygon boundary to circle centre.
            var cx:Number = circle.px;
            var cy:Number = circle.py;
            var bestD:Number = 1e30;
            var bnx:Number = 0;
            var bny:Number = -1;
            var inside:Boolean = pointInPoly(cx, cy, pxs, n);
            for (var e:int = 0; e < n; e++) {
                var p1:Object = pxs[e];
                var p2:Object = pxs[(e + 1) % n];
                var ex:Number = p2.x - p1.x;
                var ey:Number = p2.y - p1.y;
                var len2:Number = ex * ex + ey * ey;
                var t:Number = 0;
                if (len2 > 0.0001) {
                    t = ((cx - p1.x) * ex + (cy - p1.y) * ey) / len2;
                    if (t < 0) { t = 0; }
                    if (t > 1) { t = 1; }
                }
                var qx:Number = p1.x + ex * t;
                var qy:Number = p1.y + ey * t;
                var ddx:Number = cx - qx;
                var ddy:Number = cy - qy;
                var d:Number = fSqrt(ddx * ddx + ddy * ddy);
                if (inside) {
                    // Edge normal (outward): perpendicular, sign by winding.
                    var en:Number = fSqrt(len2);
                    if (en > 0.0001) {
                        var nx0:Number = -ey / en;
                        var ny0:Number = ex / en;
                        // Orient outward: normal should point away from centroid.
                        var mx:Number = (p1.x + p2.x) / 2 - poly.px;
                        var my:Number = (p1.y + p2.y) / 2 - poly.py;
                        if (nx0 * mx + ny0 * my < 0) {
                            nx0 = -nx0;
                            ny0 = -ny0;
                        }
                        // Signed distance from centre to edge along normal.
                        var sd:Number = (cx - p1.x) * nx0 + (cy - p1.y) * ny0;
                        if (sd < bestD) {
                            bestD = sd;
                            bnx = nx0;
                            bny = ny0;
                        }
                    }
                } else if (d < bestD) {
                    bestD = d;
                    if (d > 0.0001) {
                        bnx = ddx / d;
                        bny = ddy / d;
                    }
                }
            }
            if (inside) {
                // Depth = r + distance to nearest edge (centre inside).
                return { nx: bnx, ny: bny, depth: r + bestD };
            }
            var depth:Number = r - bestD;
            if (depth <= 0) {
                return null;
            }
            return { nx: bnx, ny: bny, depth: depth };
        }

        private static function pointInPoly(x:Number, y:Number, pts:Object, n:int):Boolean {
            var inside:Boolean = false;
            var j:int = n - 1;
            for (var i:int = 0; i < n; i++) {
                var pi:Object = pts[i];
                var pj:Object = pts[j];
                if ((pi.y > y) != (pj.y > y) && x < (pj.x - pi.x) * (y - pi.y) / (pj.y - pi.y) + pi.x) {
                    inside = !inside;
                }
                j = i;
            }
            return inside;
        }

        private static function polyPoly(a:Object, b:Object):Object {
            var na:int = polyPoints(a, a.ptsA);
            var nb:int = polyPoints(b, b.ptsB);
            if (na < 3 || nb < 3) {
                return null;
            }
            var pa:Object = worldPoints(a, na, "ptsA");
            var pb:Object = worldPoints(b, nb, "ptsB");
            var minOverlap:Number = 1e30;
            var nx:Number = 0;
            var ny:Number = 0;
            // Axes from both polygons' edges.
            for (var pass:int = 0; pass < 2; pass++) {
                var pts:Object = pass == 0 ? pa : pb;
                var cnt:int = pass == 0 ? na : nb;
                for (var i:int = 0; i < cnt; i++) {
                    var p1:Object = pts[i];
                    var p2:Object = pts[(i + 1) % cnt];
                    var ex:Number = p2.x - p1.x;
                    var ey:Number = p2.y - p1.y;
                    var len:Number = fSqrt(ex * ex + ey * ey);
                    if (len < 0.0001) {
                        continue;
                    }
                    var ax:Number = -ey / len;
                    var ay:Number = ex / len;
                    var minA:Number = 1e30;
                    var maxA:Number = -1e30;
                    for (var k:int = 0; k < na; k++) {
                        var d:Number = pa[k].x * ax + pa[k].y * ay;
                        if (d < minA) { minA = d; }
                        if (d > maxA) { maxA = d; }
                    }
                    var minB:Number = 1e30;
                    var maxB:Number = -1e30;
                    for (var k2:int = 0; k2 < nb; k2++) {
                        var d2:Number = pb[k2].x * ax + pb[k2].y * ay;
                        if (d2 < minB) { minB = d2; }
                        if (d2 > maxB) { maxB = d2; }
                    }
                    var overlap:Number = fMin(maxA, maxB) - fMax(minA, minB);
                    if (overlap <= 0) {
                        return null;
                    }
                    if (overlap < minOverlap) {
                        minOverlap = overlap;
                        // Orient normal from b toward a: check centroid side.
                        var cax:Number = (maxA + minA) / 2;
                        var cbx:Number = (maxB + minB) / 2;
                        if (cax >= cbx) {
                            nx = ax;
                            ny = ay;
                        } else {
                            nx = -ax;
                            ny = -ay;
                        }
                    }
                }
            }
            return { nx: nx, ny: ny, depth: minOverlap };
        }

        /// Number of polygon vertices for a body (rect -> 4, poly -> npts).
        private static function polyPoints(b:Object, cache:Object):int {
            if (b.shape == 2 && b.npts >= 3) {
                return b.npts;
            }
            return 4;
        }

        /// World-space polygon vertices; rects are generated from w/h.
        private static function worldPoints(b:Object, n:int, cacheName:String):Object {
            var cache:Object = b[cacheName];
            if (cache != null) {
                return cache;
            }
            cache = {};
            if (b.shape == 2 && b.npts >= 3) {
                for (var i:int = 0; i < n; i++) {
                    var p:Object = b.pts[i];
                    cache[i] = { x: b.px + p[0], y: b.py + p[1] };
                }
            } else {
                var hw:Number = extentW(b) / 2;
                var hh:Number = extentH(b) / 2;
                cache[0] = { x: b.px - hw, y: b.py - hh };
                cache[1] = { x: b.px + hw, y: b.py - hh };
                cache[2] = { x: b.px + hw, y: b.py + hh };
                cache[3] = { x: b.px - hw, y: b.py + hh };
            }
            b[cacheName] = cache;
            return cache;
        }

        private static function invalidatePoints(b:Object):void {
            b.ptsA = null;
            b.ptsB = null;
            b.ptsWorld = null;
        }

        private static function resolvePair(a:Object, b:Object):void {
            var c:Object = collide(a, b);
            if (c == null) {
                return;
            }
            a.px += c.nx * c.depth;
            a.py += c.ny * c.depth;
            var vn:Number = a.vx * c.nx + a.vy * c.ny;
            if (vn < 0) {
                a.vx -= (1.0 + a.bo) * vn * c.nx;
                a.vy -= (1.0 + a.bo) * vn * c.ny;
                var tx:Number = -c.ny;
                var ty:Number = c.nx;
                var vt:Number = a.vx * tx + a.vy * ty;
                var fr:Number = a.fr;
                if (fr > 1) {
                    fr = 1;
                }
                a.vx -= vt * fr * tx;
                a.vy -= vt * fr * ty;
                if (c.ny < -0.5) {
                    a.grounded = true;
                }
            }
            invalidatePoints(a);
            invalidatePoints(b);
        }

        /// Enter/exit tracking over the pre-resolution contact set computed in
        /// stepPhysics phase 2a (post-integration positions, like the preview).
        private static function updateContacts(current:Object, currentKeys:Object, currentCount:int):void {
            // Exits: previous keys absent from the current set.
            for (var e:int = 0; e < contactCount; e++) {
                var oldKey:String = contactKeys[e];
                if (oldKey == null) {
                    continue;
                }
                if (current[oldKey] != true) {
                    fireContactEvent(false, oldKey);
                }
            }
            // Enters: current keys absent from the previous set.
            for (var n:int = 0; n < currentCount; n++) {
                var newKey:String = currentKeys[n];
                if (contacts[newKey] != true) {
                    fireContactEvent(true, newKey);
                }
            }
            contacts = current;
            contactKeys = currentKeys;
            contactCount = currentCount;
        }

        private static function pairKey(a:String, b:String):String {
            if (a < b) {
                return a + "<" + b;
            }
            return b + "<" + a;
        }

        private static function fireContactEvent(enter:Boolean, key:String):void {
            var sep:int = key.indexOf("<");
            var a:String = key.substring(0, sep);
            var b:String = key.substring(sep + 1);
            eventInfo = { kind: enter ? "collisionEnter" : "collisionExit", scene: sceneName, a: a, b: b, key: 0, data: null };
            emit(enter ? "nova:collisionEnter" : "nova:collisionExit", a + "<>" + b);
        }

        /// Boolean overlap used by contact tracking (cheap broad+SAT check).
        private static function overlapTest(ca:Object, cb:Object):Boolean {
            // Broad phase on AABBs first.
            var aMinX:Number = ca.px - ca.hw;
            var aMaxX:Number = ca.px + ca.hw;
            var aMinY:Number = ca.py - ca.hh;
            var aMaxY:Number = ca.py + ca.hh;
            var bMinX:Number = cb.px - cb.hw;
            var bMaxX:Number = cb.px + cb.hw;
            var bMinY:Number = cb.py - cb.hh;
            var bMaxY:Number = cb.py + cb.hh;
            if (aMaxX < bMinX || bMaxX < aMinX || aMaxY < bMinY || bMaxY < aMinY) {
                return false;
            }
            // Reuse the body-based collide by wrapping minimal records.
            var ta:Object = { px: ca.px, py: ca.py, shape: ca.shape, w: ca.w, h: ca.h, npts: ca.npts, pts: ca.pts, ptsA: null, ptsB: null, ptsWorld: null };
            var tb:Object = { px: cb.px, py: cb.py, shape: cb.shape, w: cb.w, h: cb.h, npts: cb.npts, pts: cb.pts, ptsA: null, ptsB: null, ptsWorld: null };
            return collide(ta, tb) != null;
        }

        private static function bodyCollider(b:Object):Object {
            // Circle bounding radius uses width; rect/polygon use w/h.
            var hw:Number = extentW(b) / 2;
            var hh:Number = extentH(b) / 2;
            return { px: b.px, py: b.py, shape: b.shape, w: b.w, h: b.h, npts: b.npts, pts: b.pts, hw: hw, hh: hh };
        }

        // ------------------------------------------------------------------
        // Camera
        // ------------------------------------------------------------------

        private static function stepCamera(dt:Number):void {
            if (camCfg == null) {
                // No camera config: still apply identity pan? Do nothing.
                return;
            }
            var enabled:Boolean = f(camCfg, 0) != 0;
            if (!enabled) {
                return;
            }
            // Camera spec fields: 0 enabled, 1 left, 2 top, 3 right, 4 bottom,
            // 5 zoom, 6 smoothing, 7 deadZoneX, 8 deadZoneY, 9 shake,
            // 10 follow (object name, string).
            var follow:String = camCfg[10];
            var tx:Number = camX;
            var ty:Number = camY;
            var fp:Object = null;
            if (follow != null && follow != "") {
                fp = worldPosOf(follow);
            }
            if (fp != null) {
                tx = fp.x;
                ty = fp.y;
            } else {
                tx = worldW / 2;
                ty = worldH / 2;
            }
            // Dead zone.
            var dzx:Number = f(camCfg, 7);
            var dzy:Number = f(camCfg, 8);
            if (dzx > 0 && fAbs(tx - camX) <= dzx) {
                tx = camX;
            }
            if (dzy > 0 && fAbs(ty - camY) <= dzy) {
                ty = camY;
            }
            // Smoothing (same formula as preview).
            var smooth:Number = f(camCfg, 6);
            if (smooth > 0.95) {
                smooth = 0.95;
            }
            if (smooth < 0) {
                smooth = 0;
            }
            var t:Number = CAMERA_LERP * dt * (1.0 - smooth);
            if (t > 1) {
                t = 1;
            }
            camX += (tx - camX) * t;
            camY += (ty - camY) * t;
            camZoom = f(camCfg, 5);
            if (camZoom < 0.1) {
                camZoom = 0.1;
            }
            // Bounds (same clamp/centre-on-small-world rules as preview).
            var l:Number = f(camCfg, 1);
            var tp:Number = f(camCfg, 2);
            var r:Number = f(camCfg, 3);
            var b:Number = f(camCfg, 4);
            var halfW:Number = viewW / (2 * camZoom);
            var halfH:Number = viewH / (2 * camZoom);
            if (r > l) {
                var loX:Number = l + halfW;
                var hiX:Number = r - halfW;
                if (loX <= hiX) {
                    if (camX < loX) { camX = loX; }
                    if (camX > hiX) { camX = hiX; }
                } else {
                    camX = (l + r) / 2;
                }
            }
            if (b > tp) {
                var loY:Number = tp + halfH;
                var hiY:Number = b - halfH;
                if (loY <= hiY) {
                    if (camY < loY) { camY = loY; }
                    if (camY > hiY) { camY = hiY; }
                } else {
                    camY = (tp + b) / 2;
                }
            }
            // Shake.
            var shake:Number = f(camCfg, 9);
            if (shake > 0) {
                camX += fSin(timeSec * 39.7) * shake;
                camY += fSin(timeSec * 47.3 + 1.7) * shake * 0.8;
            }
            applyCamera();
        }

        /// Resolves the WORLD position of a named child. Physics bodies are
        /// authoritative via their body record; other children use the camera
        /// base transform, refreshed whenever an external write (script,
        /// timeline, physics) changes the display value away from the last
        /// camera-applied screen position.
        private static function worldPosOf(name:String):Object {
            for (var i:int = 0; i < bodyCount; i++) {
                var b:Object = bodies[i];
                if (b != null && b.name == name) {
                    return { x: b.px, y: b.py };
                }
            }
            var c:DisplayObject = root.getChildByName(name);
            var base:Object = camBase[name];
            if (c != null && base != null && (c.x != base.sx || c.y != base.sy)) {
                return { x: c.x, y: c.y };
            }
            if (base != null) {
                return { x: base.bx, y: base.by };
            }
            if (c != null) {
                return { x: c.x, y: c.y };
            }
            return null;
        }

        /// Applies the camera to every named top-level child. For each child
        /// the "world" position is whatever was last written by game logic:
        /// if the display transform still equals the value the camera itself
        /// produced last frame, the stored base is kept; any other value is
        /// treated as a fresh world-space write. This keeps physics-driven,
        /// script-driven and static children correct without a container.
        private static function applyCamera():void {
            var n:int = root.numChildren;
            var zoom:Number = camZoom;
            for (var i:int = 0; i < n; i++) {
                var c:DisplayObject = root.getChildAt(i);
                if (c == null) {
                    continue;
                }
                var key:String = c.name;
                if (key == "") {
                    continue;
                }
                var base:Object = camBase[key];
                if (base == null) {
                    base = {
                        bx: c.x, by: c.y, bsx: c.scaleX, bsy: c.scaleY,
                        sx: c.x, sy: c.y, ssx: c.scaleX, ssy: c.scaleY
                    };
                    camBase[key] = base;
                }
                // World position: bodies are authoritative via their record;
                // other children use external-write detection.
                var wp:Object = worldPosOf(key);
                if (wp != null) {
                    base.bx = wp.x;
                    base.by = wp.y;
                } else if (c.x != base.sx || c.y != base.sy) {
                    base.bx = c.x;
                    base.by = c.y;
                }
                if (c.scaleX != base.ssx || c.scaleY != base.ssy) {
                    base.bsx = c.scaleX;
                    base.bsy = c.scaleY;
                }
                c.x = viewW / 2 + (base.bx - camX) * zoom;
                c.y = viewH / 2 + (base.by - camY) * zoom;
                c.scaleX = base.bsx * zoom;
                c.scaleY = base.bsy * zoom;
                base.sx = c.x;
                base.sy = c.y;
                base.ssx = c.scaleX;
                base.ssy = c.scaleY;
            }
        }

        // ------------------------------------------------------------------
        // Particles (ported from nova_core::particles semantics)
        // ------------------------------------------------------------------

        private static var emitters:Object = {};
        private static var emitCount:int = 0;

        /// Builds emitter runtime records for the current scene.
        private static function syncEmitters():void {
            emitters = {};
            emitCount = 0;
            if (sceneName == "") {
                return;
            }
            var table:Object = NovaGameData.EMITTERS[sceneName];
            if (table == null) {
                return;
            }
            var count:int = table.count;
            for (var i:int = 0; i < count; i++) {
                var spec:String = table[i];
                var p:Object = splitPipe(spec);
                if (p.n < 29) {
                    continue;
                }
                var e:Object = {};
                e.name = p[0];
                e.spec = p;
                e.o = null;
                e.pool = {};
                e.live = 0;
                e.acc = 0;
                e.burstDone = false;
                e.rng = 12345 + i * 7919;
                e.prevX = 0;
                e.prevY = 0;
                e.hasPrev = false;
                emitters[i] = e;
            }
            emitCount = count;
        }

        private static function clearEmitters():void {
            for (var i:int = 0; i < emitCount; i++) {
                var e:Object = emitters[i];
                if (e == null) {
                    continue;
                }
                for (var s:int = 0; s < 512; s++) {
                    var slot:Object = e.pool[s];
                    if (slot != null && slot.view != null) {
                        root.removeChild(slot.view);
                    }
                }
            }
            emitters = {};
            emitCount = 0;
        }

        private static function stepParticles(dt:Number):void {
            if (emitCount == 0 || root == null) {
                return;
            }
            for (var i:int = 0; i < emitCount; i++) {
                var e:Object = emitters[i];
                if (e == null) {
                    continue;
                }
                var obj:DisplayObject = findObject(e.name);
                if (obj == null) {
                    continue;
                }
                e.o = obj;
                updateEmitter(e, obj.x, obj.y, dt);
            }
        }

        /// Finds a display object by (sanitised) name, tolerating the
        /// `_<depth>` suffix the exporter adds to duplicate instance names.
        private static function findObject(name:String):DisplayObject {
            var obj:DisplayObject = root.getChildByName(name);
            if (obj != null) {
                return obj;
            }
            for (var k:int = 2; k <= 64; k++) {
                obj = root.getChildByName(name + "_" + k);
                if (obj != null) {
                    return obj;
                }
            }
            return null;
        }

        private static function updateEmitter(e:Object, ox:Number, oy:Number, dt:Number):void {
            var spec:Object = e.spec;
            var local:Boolean = f(spec, 28) == 0;
            // Local space: drag live particles by the emitter delta.
            if (local && e.hasPrev) {
                var dx:Number = ox - e.prevX;
                var dy:Number = oy - e.prevY;
                if (dx != 0 || dy != 0) {
                    for (var s:int = 0; s < 512; s++) {
                        var slot:Object = e.pool[s];
                        if (slot != null && slot.alive == true) {
                            slot.x += dx;
                            slot.y += dy;
                        }
                    }
                }
            }
            e.prevX = ox;
            e.prevY = oy;
            e.hasPrev = true;

            var maxP:int = parseInt(spec[27], 10);
            if (maxP < 1) {
                maxP = 1;
            }
            if (maxP > 512) {
                maxP = 512;
            }
            // Continuous emission.
            var rate:Number = f(spec, 1);
            if (rate > 0) {
                e.acc += rate * dt;
                while (e.acc >= 1.0 && e.live < maxP) {
                    e.acc -= 1.0;
                    spawnParticle(e, ox, oy, local);
                }
            }
            // One-shot burst on first update.
            if (e.burstDone != true) {
                e.burstDone = true;
                var burst:int = parseInt(spec[2], 10);
                if (burst > maxP) {
                    burst = maxP;
                }
                for (var b:int = 0; b < burst; b++) {
                    if (e.live >= maxP) {
                        break;
                    }
                    spawnParticle(e, ox, oy, local);
                }
            }

            var grav:Number = f(spec, 23);
            var drag:Number = f(spec, 24);
            for (var i:int = 0; i < 512; i++) {
                var p:Object = e.pool[i];
                if (p == null || p.alive != true) {
                    continue;
                }
                p.age += dt;
                if (p.age >= p.life) {
                    p.alive = false;
                    p.view.visible = false;
                    e.live--;
                    continue;
                }
                p.vy += grav * dt;
                var damp:Number = 1.0 - drag * dt;
                if (damp < 0) {
                    damp = 0;
                }
                p.vx *= damp;
                p.vy *= damp;
                p.x += p.vx * dt;
                p.y += p.vy * dt;
                p.rot += p.rotV * dt;
                renderParticle(e, p, ox, oy, local);
            }
        }

        private static function rand01(e:Object):Number {
            // Park-Miller LCG; product stays below 2^53 so doubles are exact.
            e.rng = (e.rng * 48271) % 2147483647;
            return e.rng / 2147483647;
        }

        private static function prange(e:Object, spec:Object, i0:int, i1:int):Number {
            var a:Number = f(spec, i0);
            var b:Number = f(spec, i1);
            return a + (b - a) * rand01(e);
        }

        private static function spawnParticle(e:Object, ox:Number, oy:Number, local:Boolean):void {
            var spec:Object = e.spec;
            // Find a free pool slot.
            var slot:Object = null;
            var slotIdx:int = -1;
            for (var i:int = 0; i < 512; i++) {
                var cand:Object = e.pool[i];
                if (cand == null) {
                    cand = {};
                    cand.view = new flash.display.Sprite();
                    cand.view.visible = false;
                    root.addChild(cand.view);
                    e.pool[i] = cand;
                }
                if (cand.alive != true) {
                    slot = cand;
                    slotIdx = i;
                    break;
                }
            }
            if (slot == null) {
                return;
            }
            var speed:Number = prange(e, spec, 19, 20);
            var dirDeg:Number = f(spec, 21);
            var spreadDeg:Number = f(spec, 22);
            var angle:Number = (dirDeg + (rand01(e) - 0.5) * 2.0 * spreadDeg) * 3.141592653589793 / 180.0;
            slot.alive = true;
            slot.x = local ? 0 : ox;
            slot.y = local ? 0 : oy;
            slot.vx = speed * fCos(angle);
            slot.vy = speed * fSin(angle);
            slot.age = 0;
            slot.life = prange(e, spec, 3, 4);
            if (slot.life < 0.01) {
                slot.life = 0.01;
            }
            slot.ss = prange(e, spec, 5, 6);
            slot.se = prange(e, spec, 7, 8);
            slot.os = prange(e, spec, 9, 10);
            slot.oe = prange(e, spec, 11, 12);
            slot.csr = f(spec, 13);
            slot.csg = f(spec, 14);
            slot.csb = f(spec, 15);
            slot.cer = f(spec, 16);
            slot.ceg = f(spec, 17);
            slot.ceb = f(spec, 18);
            slot.rot = rand01(e) * 360.0;
            slot.rotV = prange(e, spec, 25, 26) * 3.141592653589793 / 180.0;
            e.live++;
            renderParticle(e, slot, ox, oy, local);
        }

        private static function renderParticle(e:Object, p:Object, ox:Number, oy:Number, local:Boolean):void {
            var t:Number = p.age / p.life;
            if (t > 1) {
                t = 1;
            }
            if (t < 0) {
                t = 0;
            }
            var size:Number = p.ss + (p.se - p.ss) * t;
            var op:Number = p.os + (p.oe - p.os) * t;
            if (op < 0) {
                op = 0;
            }
            if (op > 1) {
                op = 1;
            }
            var r:int = p.csr + (p.cer - p.csr) * t;
            var g:int = p.csg + (p.ceg - p.csg) * t;
            var b:int = p.csb + (p.ceb - p.csb) * t;
            if (size < 0.5) {
                size = 0.5;
            }
            var v = p.view;
            v.x = local ? ox + p.x : p.x;
            v.y = local ? oy + p.y : p.y;
            v.rotation = p.rot;
            var half:Number = size / 2;
            v.graphics.clear();
            v.graphics.beginFill((r << 16) | (g << 8) | b, op);
            v.graphics.drawRect(-half, -half, size, size);
            v.graphics.endFill();
        }

        // ------------------------------------------------------------------
        // String / number helpers (no Math in the AS3 subset)
        // ------------------------------------------------------------------

        /// Splits "a|b|c" into Object {n, 0..n-1}.
        private static function splitPipe(s:String):Object {
            var out:Object = { n: 0 };
            var start:int = 0;
            var idx:int = 0;
            while (true) {
                var bar:int = s.indexOf("|", start);
                var piece:String;
                if (bar < 0) {
                    piece = s.substring(start);
                } else {
                    piece = s.substring(start, bar);
                }
                out[idx] = piece;
                idx++;
                if (bar < 0) {
                    break;
                }
                start = bar + 1;
            }
            out.n = idx;
            return out;
        }

        /// Splits "a;b;c" into Object {n, 0..n-1}.
        private static function splitSemi(s:String):Object {
            var out:Object = { n: 0 };
            var start:int = 0;
            var idx:int = 0;
            while (true) {
                var bar:int = s.indexOf(";", start);
                var piece:String;
                if (bar < 0) {
                    piece = s.substring(start);
                } else {
                    piece = s.substring(start, bar);
                }
                out[idx] = piece;
                idx++;
                if (bar < 0) {
                    break;
                }
                start = bar + 1;
            }
            out.n = idx;
            return out;
        }

        /// Splits "x,y" into Object {0:x, 1:y}.
        private static function splitComma(s:String):Object {
            var out:Object = { n: 2 };
            var comma:int = s.indexOf(",");
            if (comma < 0) {
                out[0] = s;
                out[1] = "0";
            } else {
                out[0] = s.substring(0, comma);
                out[1] = s.substring(comma + 1);
            }
            return out;
        }

        /// Numeric field of a pipe-split spec; NaN when not numeric.
        private static function f(parts:Object, idx:int):Number {
            if (idx >= parts.n) {
                return 0;
            }
            return parseFloat(parts[idx]);
        }

        private static function fAbs(x:Number):Number {
            return x < 0 ? -x : x;
        }

        private static function fMin(a:Number, b:Number):Number {
            return a < b ? a : b;
        }

        private static function fMax(a:Number, b:Number):Number {
            return a > b ? a : b;
        }

        private static function fSqrt(x:Number):Number {
            if (x <= 0) {
                return 0;
            }
            var r:Number = x;
            if (r > 1) {
                r = x / 2 + 1;
            }
            for (var i:int = 0; i < 24; i++) {
                r = (r + x / r) / 2;
            }
            return r;
        }

        private static function fSin(x:Number):Number {
            // Range reduction to [-PI, PI], then Taylor (7 terms).
            var pi:Number = 3.141592653589793;
            var twoPi:Number = 6.283185307179586;
            var y:Number = x % twoPi;
            if (y > pi) {
                y -= twoPi;
            }
            if (y < -pi) {
                y += twoPi;
            }
            var y2:Number = y * y;
            var term:Number = y;
            var sum:Number = y;
            // sin(y) = y - y^3/3! + y^5/5! - ...
            var sign:Number = -1;
            var fact:Number = 6;
            var pow:Number = y2;
            for (var i:int = 1; i <= 6; i++) {
                term = pow * y / fact;
                sum += sign * term;
                sign = -sign;
                fact *= (2 * i + 2) * (2 * i + 3);
                pow *= y2;
            }
            return sum;
        }

        private static function fCos(x:Number):Number {
            return fSin(x + 1.5707963267948966);
        }
    }
}
"#;

/// Adds every missing game-kit script to `project`. Existing scripts with the
/// same virtual path are kept untouched so user edits are never overwritten.
pub fn install(project: &mut Project) {
    for (path, source) in FILES {
        if project.scripts.iter().any(|script| script.path == *path) {
            continue;
        }
        project.scripts.push(Script::new(*path, *source));
    }
}
