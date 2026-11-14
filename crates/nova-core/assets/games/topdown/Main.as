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
    import game.Camera;
    import game.Random;

    /// Top-down adventure: collect gems while a slow hunter patrols the map.
    /// World is larger than the stage; the camera follows the player.
    public dynamic class Main extends MovieClip {
        private var player:MovieClip;
        private var hunter:MovieClip;
        private var cam:Camera;
        private var hud:TextField;
        private var gems = [];
        private var gemCount:int = 0;
        private var score:int = 0;
        private var lastTime:int = 0;
        private var worldW:Number = 1400;
        private var worldH:Number = 900;

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

        private function build():void {
            player = box(28, 28, 0x6AD0FF);
            player.wx = 100;
            player.wy = 100;
            player.x = player.wx;
            player.y = player.wy;
            addChild(player);

            hunter = box(30, 30, 0xFF5A5A);
            hunter.wx = 1200;
            hunter.wy = 800;
            hunter.x = hunter.wx;
            hunter.y = hunter.wy;
            addChild(hunter);

            var i:int;
            for (i = 0; i < 10; i++) {
                var g:MovieClip = box(18, 18, 0x7DFF8A);
                g.wx = 80 + Random.range(worldW - 160);
                g.wy = 80 + Random.range(worldH - 160);
                g.x = g.wx;
                g.y = g.wy;
                addChild(g);
                gems.push(g);
                gemCount = gemCount + 1;
            }

            cam = new Camera();
            cam.follow(player);
            cam.x = player.wx;
            cam.y = player.wy;

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
            hud.text = "Edelsteine: " + score + " / " + gemCount;
        }

        private function update(event:Event):void {
            var now:int = getTimer();
            var dt:Number = (now - lastTime) / 1000;
            lastTime = now;
            if (dt <= 0 || dt > 0.1) {
                dt = 0.016;
            }
            Input.poll();

            var dx:Number = 0;
            var dy:Number = 0;
            var speed:Number = 260;
            if (Input.down(Keyboard.LEFT)) { dx = dx - 1; }
            if (Input.down(Keyboard.RIGHT)) { dx = dx + 1; }
            if (Input.down(Keyboard.UP)) { dy = dy - 1; }
            if (Input.down(Keyboard.DOWN)) { dy = dy + 1; }
            if (dx != 0 || dy != 0) {
                // Normalise diagonal movement
                var len:Number = 1;
                if (dx != 0 && dy != 0) {
                    len = 1.4142;
                }
                player.wx = player.wx + dx / len * speed * dt;
                player.wy = player.wy + dy / len * speed * dt;
            }
            if (player.wx < 20) { player.wx = 20; }
            if (player.wx > worldW - 20) { player.wx = worldW - 20; }
            if (player.wy < 20) { player.wy = 20; }
            if (player.wy > worldH - 20) { player.wy = worldH - 20; }

            // Hunter chases axis-wise at a slightly lower speed
            var hSpeed:Number = 150 * dt;
            var hx:Number = player.wx - hunter.wx;
            var hy:Number = player.wy - hunter.wy;
            if (hx > 4) { hunter.wx = hunter.wx + hSpeed; }
            if (hx < -4) { hunter.wx = hunter.wx - hSpeed; }
            if (hy > 4) { hunter.wy = hunter.wy + hSpeed; }
            if (hy < -4) { hunter.wy = hunter.wy - hSpeed; }

            // Gem pickup
            for (var i:int = 0; i < gems.length; i++) {
                var g = gems[i];
                if (g.visible != false && Collision.circle(
                    player.wx, player.wy, 20, g.wx, g.wy, 14
                )) {
                    g.visible = false;
                    score = score + 1;
                }
            }

            // Caught by the hunter -> back to start, gems stay collected
            if (Collision.aabb(
                player.wx - 14, player.wy - 14, 28, 28,
                hunter.wx - 15, hunter.wy - 15, 30, 30
            )) {
                player.wx = 100;
                player.wy = 100;
            }
            updateHud();

            cam.update(dt, stage.stageWidth, stage.stageHeight, 0, 0, worldW, worldH);
            var viewCx:Number = stage.stageWidth / 2;
            var viewCy:Number = stage.stageHeight / 2;
            player.x = player.wx - cam.x + viewCx;
            player.y = player.wy - cam.y + viewCy;
            hunter.x = hunter.wx - cam.x + viewCx;
            hunter.y = hunter.wy - cam.y + viewCy;
            for (i = 0; i < gems.length; i++) {
                g = gems[i];
                if (g.visible != false) {
                    g.x = g.wx - cam.x + viewCx;
                    g.y = g.wy - cam.y + viewCy;
                }
            }
        }
    }
}
