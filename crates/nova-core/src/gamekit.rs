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
