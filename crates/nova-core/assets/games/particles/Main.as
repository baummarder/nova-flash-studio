package {
    import flash.display.MovieClip;
    import flash.display.Graphics;

    /// Particle showcase: the emitters configured on the timeline objects are
    /// simulated by the NØVA game preview. This document class only draws the
    /// static backdrop — particle systems are a preview-runtime feature and
    /// are intentionally NOT exported into the SWF (see docs/EXPORT.md).
    public dynamic class Main extends MovieClip {
        public function Main() {
            var bg:MovieClip = new MovieClip();
            var g:Graphics = bg.graphics;
            g.beginFill(0x0B0D14);
            g.drawRect(0, 0, stage.stageWidth, stage.stageHeight);
            g.endFill();
            addChild(bg);

            var floor:MovieClip = new MovieClip();
            var fg:Graphics = floor.graphics;
            fg.beginFill(0x1B2030);
            fg.drawRect(0, stage.stageHeight - 60, stage.stageWidth, 60);
            fg.endFill();
            addChild(floor);
        }
    }
}
