package {
    import flash.display.MovieClip;
    import flash.display.Graphics;
    import flash.events.Event;
    import flash.text.TextField;
    import flash.text.TextFormat;
    import flash.ui.Keyboard;
    import flash.utils.getTimer;
    import game.Input;

    /// Blank game scaffold: a movable square proving input + game kit work.
    /// Extend `update` with your own mechanics; helpers live in `game.*`.
    public dynamic class Main extends MovieClip {
        private var hero:MovieClip;
        private var hint:TextField;
        private var lastTime:int = 0;

        public function Main() {
            Input.init(stage);
            var bg:MovieClip = new MovieClip();
            var g:Graphics = bg.graphics;
            g.beginFill(0x14141E);
            g.drawRect(0, 0, stage.stageWidth, stage.stageHeight);
            g.endFill();
            addChild(bg);

            hero = new MovieClip();
            var hg:Graphics = hero.graphics;
            hg.beginFill(0x6AD0FF);
            hg.drawRect(-20, -20, 40, 40);
            hg.endFill();
            hero.x = stage.stageWidth / 2;
            hero.y = stage.stageHeight / 2;
            addChild(hero);

            hint = new TextField();
            var fmt:TextFormat = new TextFormat();
            fmt.size = 20;
            fmt.color = 0xFFFFFF;
            hint.defaultTextFormat = fmt;
            hint.x = 20;
            hint.y = 20;
            hint.text = "Pfeiltasten bewegen - dies ist dein Blank Game";
            addChild(hint);

            addEventListener(Event.ENTER_FRAME, update);
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
            if (Input.down(Keyboard.LEFT)) { dx = dx - 1; }
            if (Input.down(Keyboard.RIGHT)) { dx = dx + 1; }
            if (Input.down(Keyboard.UP)) { dy = dy - 1; }
            if (Input.down(Keyboard.DOWN)) { dy = dy + 1; }
            hero.x = hero.x + dx * 240 * dt;
            hero.y = hero.y + dy * 240 * dt;
        }
    }
}
