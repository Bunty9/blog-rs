use crate::{RenderedBlock, RenderError, Shortcode, ShortcodeArgs};
pub struct Image;
impl Shortcode for Image {
    fn name(&self) -> &'static str { "image" }
    fn render(&self, _args: &ShortcodeArgs, _body: Option<&str>) -> Result<RenderedBlock, RenderError> {
        unimplemented!("filled in Task 10")
    }
}
