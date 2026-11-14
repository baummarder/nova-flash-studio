package {
    import flash.display.MovieClip;
    import flash.display.Graphics;
    import flash.events.Event;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.ui.Keyboard;
    import flash.utils.getTimer;
    import game.Input;
    import game.Collision;
    import game.Random;

    /// Top-down racer: steer a car along a winding road, dodge traffic,
    /// survive as long as possible. Score = survival time.
    public dynamic class Main extends MovieClip {
        private var car:MovieClip;
        private var hud:TextField;
        private var traffic = [];
        private var spawnTimer:Number = 0;
        private var score:Number = 0;
        private var lastTime:int = 0;
        private var worldW:Number = 800;
        private var worldH:Number = 4800;
        private var roadLeft:Number = 200;
        private var roadRight:Number = 600;

        public function Main() {
            Input.init(stage);
            build();
            addEventListener(Event.ENTER_FRAME, update);
        }

        private function box(w:Number, h:Number, color:Number):MovieClip {
            var s:MovieClip = new MovieClip();
            var g:Graphics = s.graphics;
            g.beginFill(color);
            g.drawRect(-w / 2, -h / 2, w, h);
            g.endFill();
            return s;
        }

        private function roadCenter(y:Number):Number {
            // Gentle winding road via a triangle wave (no Math lib needed)
            var p:Number = (y * 0.004) % 1.0;
            if (p < 0) { p = p + 1; }
            var k:Number = p * 2;
            if (k > 1) { k = 2 - k; }
            return worldW / 2 + (k * 2 - 1) * 160;
        }

        private function build():void {
            car = box(36, 56, 0x4AC6FF);
            car.wx = roadCenter(0);
            car.wy = 200;
            car.x = car.wx;
            car.y = car.wy;
            addChild(car);

            hud = new TextField();
            var fmt:TextFormat = new TextFormat();
            fmt.size = 22;
            fmt.color = 0xFFFFFF;
            hud.defaultTextFormat = fmt;
            hud.x = 16;
            hud.y = 16;
            addChild(hud);
            updateHud();
        }

        private function updateHud():void {
            var meters:Number = score / 10;
            hud.text = "Strecke: " + meters + " m";
        }

        private function update(event:Event):void {
            var now:int = getTimer();
            var dt:Number = (now - lastTime) / 1000;
            lastTime = now;
            if (dt <= 0 || dt > 0.1) {
                dt = 0.016;
            }
            Input.poll();
            score = score + dt;

            // Steering: car follows the road centre plus player offset
            var steer:Number = 0;
            if (Input.down(Keyboard.LEFT)) { steer = steer - 1; }
            if (Input.down(Keyboard.RIGHT)) { steer = steer + 1; }
            var targetX:Number = roadCenter(car.wy) + steer * 160;
            car.wx = car.wx + (targetX - car.wx) * 8 * dt;
            if (car.wx < roadLeft + 24) { car.wx = roadLeft + 24; }
            if (car.wx > roadRight - 24) { car.wx = roadRight - 24; }

            // Forward speed ramps up slightly
            var speed:Number = 320 + score * 2;
            if (speed > 560) { speed = 560; }
            car.wy = car.wy + speed * dt;
            if (car.wy > worldH) {
                car.wy = 200;
                score = score + 50; // lap bonus
            }

            // Spawn traffic ahead
            spawnTimer = spawnTimer - dt;
            if (spawnTimer <= 0 && traffic.length < 6) {
                spawnTimer = 0.7 + Random.range(0.8);
                var t:MovieClip = box(30, 44, 0xFFB03A);
                t.wx = roadCenter(0) + (Random.range(2) - 1) * 140;
                t.wy = -60;
                addChild(t);
                traffic.push(t);
            }

            // Move traffic toward the player (world scrolls upward past car)
            var i:int;
            for (i = traffic.length - 1; i >= 0; i--) {
                var t = traffic[i];
                t.wy = t.wy + (speed * 0.8) * dt;
                // Wrap: cars ahead of the player wrap to the back
                if (t.wy > car.wy + 500) {
                    t.wy = car.wy - 800;
                    t.wx = roadCenter(t.wy) + (Random.range(2) - 1) * 140;
                }
                if (t.wy < -100) {
                    removeChild(t);
                    traffic.splice(i, 1);
                }
                // Collision -> crash: restart position, lose time bonus
                if (Collision.aabb(
                    car.wx - 16, car.wy - 26, 32, 52,
                    t.wx - 14, t.wy - 20, 28, 40
                )) {
                    car.wy = 200;
                    car.wx = roadCenter(200);
                    score = score - 30;
                    if (score < 0) { score = 0; }
                }
                // Draw traffic relative to a simple scrolling view:
                // we render the whole world by shifting every MovieClip.
                t.x = t.wx - camX() + stage.stageWidth / 2;
                t.y = t.wy - camY() + stage.stageHeight / 2 - 160;
            }

            car.x = car.wx - camX() + stage.stageWidth / 2;
            car.y = car.wy - camY() + stage.stageHeight / 2 - 160;
            updateHud();
        }

        // The camera is fixed on the car; helpers keep the math readable.
        private function camX():Number {
            return car.wx - stage.stageWidth / 2;
        }
        private function camY():Number {
            return car.wy - 160;
        }
    }
}
