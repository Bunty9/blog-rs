use crate::{RenderedBlock, RenderError, Shortcode, ShortcodeArgs};
pub struct Chart;
impl Shortcode for Chart {
    fn name(&self) -> &'static str { "chart" }
    fn render(&self, _args: &ShortcodeArgs, _body: Option<&str>) -> Result<RenderedBlock, RenderError> {
        unimplemented!("filled in Task 11")
    }
}
