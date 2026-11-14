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
    import game.Physics;
    import game.Camera;

    /// Side-scrolling platformer. Level geometry (ground, platforms, coins)
    /// lives on the NØVA timeline as named MovieClips; this document class drives
    /// the player, camera and pickups. The same objects carry game metadata so
    /// the editor preview simulates them too.
    public dynamic class Main extends MovieClip {
        private var player:MovieClip;
        private var cam:Camera;
        private var hud:TextField;
        private var plats = [];
        private var coins = [];
        private var score:int = 0;
        private var lastTime:int = 0;
        private var worldW:Number = 1600;
        private var worldH:Number = 360;

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

        private function collect(name:String, list:Object):void {
            var i:int = 1;
            while (true) {
                var found = getChildByName(name + i);
                if (found == null) {
                    break;
                }
                var entry = {o: found, hx: found.x, hy: found.y};
                list.push(entry);
                i = i + 1;
            }
        }

        private function build():void {
            collect("Plat", plats);
            collect("Coin", coins);

            player = box(26, 34, 0xFFD24A);
            player.x = 80;
            player.y = 300;
            player.vx = 0;
            player.vy = 0;
            player.wx = player.x;
            player.wy = player.y;
            player.grounded = false;
            addChild(player);

            cam = new Camera();
            cam.follow(player);
            cam.x = player.x;
            cam.y = player.y;

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
            hud.text = "Muenzen: " + score;
        }

        private function applyCamera():void {
            var viewCx:Number = stage.stageWidth / 2;
            var i:int;
            for (i = 0; i < plats.length; i++) {
                var p = plats[i];
                p.o.x = p.hx - cam.x + viewCx;
            }
            for (i = 0; i < coins.length; i++) {
                var c = coins[i];
                if (c.o.visible != false) {
                    c.o.x = c.hx - cam.x + viewCx;
                    c.o.y = c.hy;
                }
            }
            player.x = player.wx - cam.x + viewCx;
            player.y = player.wy;
        }

        private function update(event:Event):void {
            var now:int = getTimer();
            var dt:Number = (now - lastTime) / 1000;
            lastTime = now;
            if (dt <= 0 || dt > 0.1) {
                dt = 0.016;
            }
            Input.poll();

            // Horizontal movement
            var dir:int = 0;
            if (Input.down(Keyboard.LEFT)) {
                dir = dir - 1;
            }
            if (Input.down(Keyboard.RIGHT)) {
                dir = dir + 1;
            }
            player.vx = dir * 240;
            if (Input.pressed(Keyboard.SPACE) && player.grounded == true) {
                player.vy = -520;
                player.grounded = false;
            }

            // Integrate (player gravity 1.0)
            player.wx = player.wx + player.vx * dt;
            player.wy = player.wy + player.vy * dt;
            player.vy = player.vy + 1500 * dt;

            // World bounds
            if (player.wx < 20) { player.wx = 20; }
            if (player.wx > worldW - 20) { player.wx = worldW - 20; }

            // Platform collision: land on top surfaces only
            player.grounded = false;
            for (var i:int = 0; i < plats.length; i++) {
                var p = plats[i];
                if (Collision.aabb(
                    player.wx - player.width / 2, player.wy - player.height / 2,
                    player.width, player.height,
                    p.hx - p.o.width / 2, p.hy - p.o.height / 2,
                    p.o.width, p.o.height
                )) {
                    // Only resolve when falling onto the top
                    if (player.vy > 0 && player.wy - player.height / 2 < p.hy) {
                        player.wy = p.hy - p.o.height / 2 - player.height / 2;
                        player.vy = 0;
                        player.grounded = true;
                    }
                }
            }

            // Fell off the world -> respawn
            if (player.wy > worldH + 60) {
                player.wx = 80;
                player.wy = 120;
                player.vx = 0;
                player.vy = 0;
            }

            // Coins
            for (i = 0; i < coins.length; i++) {
                var c = coins[i];
                if (c.o.visible != false && Collision.aabb(
                    player.wx - player.width / 2, player.wy - player.height / 2,
                    player.width, player.height,
                    c.hx - c.o.width / 2, c.hy - c.o.height / 2,
                    c.o.width, c.o.height
                )) {
                    c.o.visible = false;
                    score = score + 1;
                }
            }
            updateHud();

            // Camera follows the player inside world bounds
            cam.update(dt, stage.stageWidth, stage.stageHeight, 0, 0, worldW, worldH);
            applyCamera();
        }
    }
}
