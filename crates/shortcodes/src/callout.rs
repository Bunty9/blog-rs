use crate::{RenderedBlock, RenderError, Shortcode, ShortcodeArgs};
pub struct Callout;
impl Shortcode for Callout {
    fn name(&self) -> &'static str { "callout" }
    fn paired(&self) -> bool { true }
    fn render(&self, _args: &ShortcodeArgs, _body: Option<&str>) -> Result<RenderedBlock, RenderError> {
        unimplemented!("filled in Task 8")
    }
}
