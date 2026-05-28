use crate::{RenderedBlock, RenderError, Shortcode, ShortcodeArgs};
pub struct Embed;
impl Shortcode for Embed {
    fn name(&self) -> &'static str { "embed" }
    fn render(&self, _args: &ShortcodeArgs, _body: Option<&str>) -> Result<RenderedBlock, RenderError> {
        unimplemented!("filled in Task 13")
    }
}
