use crate::{RenderedBlock, RenderError, Shortcode, ShortcodeArgs};
pub struct Playable;
impl Shortcode for Playable {
    fn name(&self) -> &'static str { "playable" }
    fn render(&self, _args: &ShortcodeArgs, _body: Option<&str>) -> Result<RenderedBlock, RenderError> {
        unimplemented!("filled in Task 13")
    }
}
