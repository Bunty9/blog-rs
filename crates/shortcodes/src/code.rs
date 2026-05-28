use crate::{RenderedBlock, RenderError, Shortcode, ShortcodeArgs};
pub struct Code;
impl Shortcode for Code {
    fn name(&self) -> &'static str { "code" }
    fn paired(&self) -> bool { true }
    fn render(&self, _args: &ShortcodeArgs, _body: Option<&str>) -> Result<RenderedBlock, RenderError> {
        unimplemented!("filled in Task 9")
    }
}
