use crate::{RenderedBlock, RenderError, Shortcode, ShortcodeArgs};
pub struct Animate;
impl Shortcode for Animate {
    fn name(&self) -> &'static str { "animate" }
    fn paired(&self) -> bool { true }
    fn render(&self, _args: &ShortcodeArgs, _body: Option<&str>) -> Result<RenderedBlock, RenderError> {
        unimplemented!("filled in Task 12")
    }
}
