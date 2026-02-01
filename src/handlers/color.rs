use smithay::{
    delegate_color_management, delegate_color_representation,
    output::Output,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    wayland::color::{
        management::{ColorManagementHandler, ColorManagementState, ImageDescription},
        representation::{ColorRepresentationHandler, ColorRepresentationState},
    },
};

use crate::{
    niri::State,
    render_helpers::color_manage::{Colorimetry, OutputColorimetry},
};

impl ColorManagementHandler for State {
    fn color_management_state(&mut self) -> &mut ColorManagementState {
        &mut self.niri.color_management_state
    }

    fn verify_icc(&mut self, icc_data: &[u8]) -> bool {
        false
    }

    fn description_for_output(&mut self, output: &Output) -> ImageDescription {
        let colorimetry = output
            .user_data()
            .get::<OutputColorimetry>()
            .map(|colorimetry| colorimetry.colorimetry.lock().unwrap().clone())
            .unwrap_or(Colorimetry::srgb_sdr());

        self.color_management_state()
            .build_description((&colorimetry).into())
    }

    fn preferred_description_for_surface(&mut self, surface: &WlSurface) -> ImageDescription {
        let colorimetry = self
            .niri
            .output_for_root(surface)
            .inspect(|x| log::info!("surface output: {:?}", x))
            .and_then(|output| {
                output
                    .user_data()
                    .get::<OutputColorimetry>()
                    .map(|colorimetry| colorimetry.colorimetry.lock().unwrap().clone())
            })
            .unwrap_or(Colorimetry::srgb_sdr());

        self.color_management_state()
            .build_description((&colorimetry).into())
    }
}

impl ColorRepresentationHandler for State {
    fn color_representation_state(&mut self) -> &mut ColorRepresentationState {
        &mut self.niri.color_representation_state
    }
}

delegate_color_representation!(State);
delegate_color_management!(State);
