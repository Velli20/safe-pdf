use pdf_content_stream::pdf_operator_backend::ColorOps;

use crate::{canvas_backend::CanvasBackend, pdf_canvas::PdfCanvas};
use pdf_graphics::color::Color;

impl<'a, U, T: CanvasBackend<ImageType = U>> ColorOps for PdfCanvas<'a, T, U> {
    fn set_stroking_color_space(&mut self, _name: &str) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.pattern = None;

        Ok(())
    }

    fn set_non_stroking_color_space(&mut self, _name: &str) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.pattern = None;

        Ok(())
    }

    fn set_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        println!("set_stroking_color {:?}", components);
        Ok(())
    }

    fn set_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &str,
    ) -> Result<(), Self::ErrorType> {
        let Some(pattern) = self
            .page
            .resources
            .as_ref()
            .and_then(|r| r.patterns.get(pattern_name))
        else {
            println!("Pattern not found {:?}", pattern_name);
            panic!()
        };

        println!(
            "set_stroking_color_extended {:?} {:?}",
            components, pattern_name
        );
        self.current_state_mut()?.pattern = Some(pattern);

        Ok(())
    }

    fn set_non_stroking_color(&mut self, _components: &[f32]) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn set_non_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: &str,
    ) -> Result<(), Self::ErrorType> {
        let Some(pattern) = self
            .page
            .resources
            .as_ref()
            .and_then(|r| r.patterns.get(pattern_name))
        else {
            println!("Pattern not found {:?}", pattern_name);
            panic!()
        };
        println!(
            "set_non_stroking_color_extended {:?} {:?}",
            components, pattern_name
        );
        self.current_state_mut()?.pattern = Some(pattern);
        Ok(())
    }

    fn set_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        println!("Set stroking gray {:?}", gray);
        Ok(())
    }

    fn set_non_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        println!("Non stroking gray {:?}", gray);
        Ok(())
    }

    fn set_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.stroke_color = Color::from_rgb(r, g, b);
        self.current_state_mut()?.pattern = None;

        Ok(())
    }

    fn set_non_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.fill_color = Color::from_rgb(r, g, b);
        self.current_state_mut()?.pattern = None;

        Ok(())
    }

    fn set_stroking_cmyk(
        &mut self,
        _c: f32,
        _m: f32,
        _y: f32,
        _k: f32,
    ) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_non_stroking_cmyk(
        &mut self,
        _c: f32,
        _m: f32,
        _y: f32,
        _k: f32,
    ) -> Result<(), Self::ErrorType> {
        todo!()
    }
}
