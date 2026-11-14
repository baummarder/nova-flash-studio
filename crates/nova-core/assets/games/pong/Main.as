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

    public dynamic class Main extends MovieClip {
        private var player:MovieClip;
        private var cpu:MovieClip;
        private var ball:MovieClip;
        private var score:TextField;
        private var pScore:int = 0;
        private var cScore:int = 0;
        private var lastTime:int = 0;
        private var serveDir:int = 1;

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
            addChild(s);
            return s;
        }

        private function build():void {
            player = box(14, 80, 0xE8E8E8);
            player.x = 40;
            player.y = stage.stageHeight / 2;
            cpu = box(14, 80, 0xE86A6A);
            cpu.x = stage.stageWidth - 40;
            cpu.y = stage.stageHeight / 2;
            ball = box(14, 14, 0xFFFFFF);
            serve(1);
            score = new TextField();
            var fmt:TextFormat = new TextFormat();
            fmt.size = 28;
            fmt.color = 0xFFFFFF;
            score.defaultTextFormat = fmt;
            score.x = stage.stageWidth / 2 - 40;
            score.y = 16;
            addChild(score);
        }

        private function serve(dir:int):void {
            serveDir = dir;
            ball.x = stage.stageWidth / 2;
            ball.y = stage.stageHeight / 2;
            ball.vx = 260 * dir;
            ball.vy = (Random.range(2) - 1) * 160;
            if (ball.vy == 0) {
                ball.vy = 80;
            }
        }

        private function update(event:Event):void {
            var now:int = getTimer();
            var dt:Number = (now - lastTime) / 1000;
            lastTime = now;
            if (dt <= 0 || dt > 0.1) {
                dt = 0.016;
            }
            Input.poll();

            // Player paddle
            if (Input.down(Keyboard.UP)) {
                player.y = player.y - 320 * dt;
            }
            if (Input.down(Keyboard.DOWN)) {
                player.y = player.y + 320 * dt;
            }
            if (player.y < 40 + player.height / 2) {
                player.y = 40 + player.height / 2;
            }
            if (player.y > stage.stageHeight - 40 - player.height / 2) {
                player.y = stage.stageHeight - 40 - player.height / 2;
            }

            // CPU paddle: chase the ball with capped speed
            var targetY:Number = ball.y;
            var diff:Number = targetY - cpu.y;
            var step:Number = 260 * dt;
            if (diff > step) { cpu.y = cpu.y + step; }
            else if (diff < -step) { cpu.y = cpu.y - step; }
            else { cpu.y = targetY; }
            if (cpu.y < 40 + cpu.height / 2) { cpu.y = 40 + cpu.height / 2; }
            if (cpu.y > stage.stageHeight - 40 - cpu.height / 2) { cpu.y = stage.stageHeight - 40 - cpu.height / 2; }

            // Ball
            ball.x = ball.x + ball.vx * dt;
            ball.y = ball.y + ball.vy * dt;

            // Walls
            if (ball.y < 30 + ball.height / 2) {
                ball.y = 30 + ball.height / 2;
                ball.vy = -ball.vy;
            }
            if (ball.y > stage.stageHeight - 30 - ball.height / 2) {
                ball.y = stage.stageHeight - 30 - ball.height / 2;
                ball.vy = -ball.vy;
            }

            // Paddle collisions
            if (Collision.objectsOverlap(ball, player) && ball.vx < 0) {
                ball.vx = -ball.vx * 1.05;
                ball.vy = ball.vy + (ball.y - player.y) * 4;
                if (ball.vx > 520) { ball.vx = 520; }
            }
            if (Collision.objectsOverlap(ball, cpu) && ball.vx > 0) {
                ball.vx = -ball.vx * 1.05;
                ball.vy = ball.vy + (ball.y - cpu.y) * 4;
                if (ball.vx < -520) { ball.vx = -520; }
            }

            // Score
            if (ball.x < -30) {
                cScore = cScore + 1;
                serve(-1);
            }
            if (ball.x > stage.stageWidth + 30) {
                pScore = pScore + 1;
                serve(1);
            }
            score.text = pScore + " : " + cScore;
        }
    }
}
