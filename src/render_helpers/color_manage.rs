use std::sync::{Arc, Mutex};

use glam::{Mat3, Vec3};
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{
    GlesError, GlesFrame, GlesRenderer, GlesTexProgram, Uniform,
};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::reexports::wayland_protocols::wp::color_management::v1::server::wp_color_manager_v1::{self, Primaries};
use smithay::utils::user_data::UserDataMap;
use smithay::utils::{Buffer, Physical, Rectangle, Scale, Transform};
use smithay::wayland::color::management::{
    ImageDescriptionContents, Luminance, MasteringLuminance, ParametricPrimaries, PrimariesEnum, TransferFunctionEnum
};

use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};
use crate::render_helpers::renderer::{AsGlesFrame as _, NiriRenderer};
use crate::render_helpers::shaders::{mat3_uniform, Shaders};

#[derive(Debug, Clone)]
pub struct OutputColorimetry {
    pub colorimetry: Arc<Mutex<Colorimetry>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TransferFunction {
    Srgb,
    St2084Pq,
    Gamma22,
}

impl TransferFunction {
    pub fn to_uniform(self) -> f32 {
        match self {
            Self::Srgb => 1f32,
            Self::St2084Pq => 2f32,
            Self::Gamma22 => 12.2f32,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ColorimetryLuminance {
    pub min: f32,
    pub max: f32,
    pub reference: f32,
}

#[derive(Debug, Copy, Clone)]
pub struct Colorimetry {
    pub tf: TransferFunction,
    pub coordinates: ColorspaceCoordinates,
    pub luminances: Option<ColorimetryLuminance>,
    pub mastering_primaries: Option<ColorspaceCoordinates>,
    pub mastering_luminances: Option<(f32, f32)>,
    pub mastering_max_cll: Option<f32>,
    pub mastering_max_fall: Option<f32>,
}

impl Into<ImageDescriptionContents> for &Colorimetry {
    fn into(self) -> ImageDescriptionContents {
        let tf = match self.tf {
            TransferFunction::Srgb => {
                TransferFunctionEnum::Named(wp_color_manager_v1::TransferFunction::Srgb)
            }
            TransferFunction::Gamma22 => {
                TransferFunctionEnum::Named(wp_color_manager_v1::TransferFunction::Gamma22)
            }
            TransferFunction::St2084Pq => {
                TransferFunctionEnum::Named(wp_color_manager_v1::TransferFunction::St2084Pq)
            }
        };
        let primaries = PrimariesEnum::Parametric((&self.coordinates).into());
        let luminances = self.luminances.map(|luminances| Luminance {
            min: (luminances.min * 10_000.0 + 0.5) as u32,
            max: (luminances.max + 0.5) as u32,
            reference: (luminances.reference + 0.5) as u32,
        });
        let target_primaries = self
            .mastering_primaries
            .map(|primaries| (&primaries).into());
        let target_luminance = self
            .mastering_luminances
            .map(|(min, max)| MasteringLuminance {
                min: (min * 1_000.0 + 0.5) as u32,
                max: (max + 0.5) as u32,
            });
        let max_cll = self.mastering_max_cll.map(|cll| (cll + 0.5) as u32);
        let max_fall = self.mastering_max_fall.map(|fall| (fall + 0.5) as u32);
        ImageDescriptionContents::Parametric {
            tf,
            primaries,
            luminances,
            target_primaries,
            target_luminance,
            max_cll,
            max_fall,
        }
    }
}

impl Into<Colorimetry> for &ImageDescriptionContents {
    fn into(self) -> Colorimetry {
        match self {
            ImageDescriptionContents::ICC(_icc) => {
                log::info!("ICC");
                Colorimetry::srgb_sdr()
            }
            ImageDescriptionContents::Parametric {
                tf,
                primaries,
                luminances,
                target_primaries,
                target_luminance,
                max_cll,
                max_fall,
            } => {
                let tf = match tf {
                    TransferFunctionEnum::Named(wp_color_manager_v1::TransferFunction::Srgb) => {
                        Some(TransferFunction::Srgb)
                    }
                    TransferFunctionEnum::Named(wp_color_manager_v1::TransferFunction::Gamma22) => {
                        Some(TransferFunction::Gamma22)
                    }
                    TransferFunctionEnum::Named(
                        wp_color_manager_v1::TransferFunction::St2084Pq,
                    ) => Some(TransferFunction::St2084Pq),
                    TransferFunctionEnum::Named(_) => None,
                    TransferFunctionEnum::Power(_) => None,
                }
                .unwrap();
                let coordinates = match primaries {
                    PrimariesEnum::Named(Primaries::Srgb) => {
                        Some(get_coordinates(MatrixCoefficients::Srgb))
                    }
                    PrimariesEnum::Named(Primaries::Bt2020) => {
                        Some(get_coordinates(MatrixCoefficients::Rec2020))
                    }
                    PrimariesEnum::Named(Primaries::DciP3) => {
                        Some(get_coordinates(MatrixCoefficients::DciP3))
                    }
                    PrimariesEnum::Named(Primaries::DisplayP3) => {
                        Some(get_coordinates(MatrixCoefficients::DisplayP3))
                    }
                    PrimariesEnum::Named(Primaries::Cie1931Xyz) => {
                        Some(get_coordinates(MatrixCoefficients::Cie1931Xyz))
                    }
                    PrimariesEnum::Named(_) => None,
                    PrimariesEnum::Parametric(parametric_primaries) => {
                        Some(parametric_primaries.into())
                    }
                }
                .unwrap();
                let luminances = luminances.map(|x| ColorimetryLuminance {
                    min: x.min as f32 / 10_000.0,
                    max: x.max as f32,
                    reference: x.reference as f32,
                });
                let mastering_primaries = target_primaries.map(|primaries| (&primaries).into());
                let mastering_luminances =
                    target_luminance.map(|lum| (lum.min as f32 / 1_000.0, lum.max as f32));
                let mastering_max_cll = max_cll.map(|cll| cll as f32);
                let mastering_max_fall = max_fall.map(|fall| fall as f32);
                Colorimetry {
                    tf,
                    coordinates,
                    luminances,
                    mastering_primaries,
                    mastering_luminances,
                    mastering_max_cll,
                    mastering_max_fall,
                }
            }
        }
    }
}

impl Default for Colorimetry {
    fn default() -> Self {
        Self {
            tf: TransferFunction::Srgb,
            coordinates: get_coordinates(MatrixCoefficients::Srgb),
            luminances: Default::default(),
            mastering_primaries: Default::default(),
            mastering_luminances: Default::default(),
            mastering_max_cll: Default::default(),
            mastering_max_fall: Default::default(),
        }
    }
}

impl Colorimetry {
    pub fn srgb_sdr() -> Colorimetry {
        Colorimetry {
            tf: TransferFunction::Srgb,
            coordinates: get_coordinates(MatrixCoefficients::Srgb),
            luminances: Some(ColorimetryLuminance {
                min: 0f32,
                max: 80f32,
                reference: 80f32,
            }),
            ..Colorimetry::default()
        }
    }

    pub fn dcip3_sdr() -> Colorimetry {
        Colorimetry {
            tf: TransferFunction::Gamma22,
            coordinates: get_coordinates(MatrixCoefficients::DciP3),
            luminances: Some(ColorimetryLuminance {
                min: 0f32,
                max: 80f32,
                reference: 80f32,
            }),
            ..Colorimetry::default()
        }
    }

    pub fn bt2020_sdr() -> Colorimetry {
        Colorimetry {
            tf: TransferFunction::Gamma22,
            coordinates: get_coordinates(MatrixCoefficients::Rec2020),
            luminances: Some(ColorimetryLuminance {
                min: 0f32,
                max: 200f32,
                reference: 200f32,
            }),
            ..Colorimetry::default()
        }
    }

    pub fn bt2020_hdr() -> Colorimetry {
        Colorimetry {
            tf: TransferFunction::St2084Pq,
            coordinates: get_coordinates(MatrixCoefficients::Rec2020),
            luminances: Some(ColorimetryLuminance {
                min: 0f32,
                max: 10000f32,
                reference: 80f32,
            }),
            ..Colorimetry::default()
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MatrixCoefficients {
    Srgb,
    Rec2020,
    DciP3,
    DisplayP3,
    Cie1931Xyz,
}

#[derive(Debug, Copy, Clone)]
pub struct ColorspaceCoordinates {
    pub r: (f32, f32),
    pub g: (f32, f32),
    pub b: (f32, f32),
    pub w: (f32, f32),
}

fn point_to_fixed(pt: (f32, f32)) -> (i32, i32) {
    (
        (pt.0 * 1_000_000.0 + 0.5) as i32,
        (pt.1 * 1_000_000.0 + 0.5) as i32,
    )
}

fn fixed_to_point(fixed: (i32, i32)) -> (f32, f32) {
    (
        (fixed.0 as f32 / 1_000_000.0),
        (fixed.1 as f32 / 1_000_000.0),
    )
}

impl Into<ParametricPrimaries> for &ColorspaceCoordinates {
    fn into(self) -> ParametricPrimaries {
        ParametricPrimaries {
            red: point_to_fixed(self.r),
            green: point_to_fixed(self.g),
            blue: point_to_fixed(self.b),
            white: point_to_fixed(self.w),
        }
    }
}

impl Into<ColorspaceCoordinates> for &ParametricPrimaries {
    fn into(self) -> ColorspaceCoordinates {
        ColorspaceCoordinates {
            r: fixed_to_point(self.red),
            g: fixed_to_point(self.green),
            b: fixed_to_point(self.blue),
            w: fixed_to_point(self.white),
        }
    }
}

pub fn get_coordinates(coefficients: MatrixCoefficients) -> ColorspaceCoordinates {
    match coefficients {
        MatrixCoefficients::Srgb => ColorspaceCoordinates {
            r: (0.6400, 0.3300),
            g: (0.3000, 0.6000),
            b: (0.1500, 0.0600),
            w: (0.3127, 0.3290),
        },
        MatrixCoefficients::Rec2020 => ColorspaceCoordinates {
            r: (0.7080, 0.2920),
            g: (0.1700, 0.7970),
            b: (0.1310, 0.0460),
            w: (0.3127, 0.3290),
        },
        MatrixCoefficients::DciP3 => ColorspaceCoordinates {
            r: (0.6800, 0.3200),
            g: (0.2650, 0.6900),
            b: (0.1500, 0.0600),
            w: (0.3140, 0.3510),
        },
        MatrixCoefficients::DisplayP3 => ColorspaceCoordinates {
            r: (0.6800, 0.3200),
            g: (0.2650, 0.6900),
            b: (0.1500, 0.0600),
            w: (0.3127, 0.3290),
        },
        MatrixCoefficients::Cie1931Xyz => ColorspaceCoordinates {
            r: (1.0000, 0.0000),
            g: (0.0000, 1.0000),
            b: (0.0000, 0.0000),
            w: (0.3333, 0.3333), // double check
        },
    }
}

fn conversion_matrix(from: &ColorspaceCoordinates) -> Mat3 {
    let from_r = Vec3::new(from.r.0, from.r.1, 1f32 - from.r.0 - from.r.1);
    let from_g = Vec3::new(from.g.0, from.g.1, 1f32 - from.g.0 - from.g.1);
    let from_b = Vec3::new(from.b.0, from.b.1, 1f32 - from.b.0 - from.b.1);
    let from_w = Vec3::new(from.w.0, from.w.1, 1f32 - from.w.0 - from.w.1);

    let from_w_xyz = (1f32 / from.w.1) * from_w;

    let from_rgb_mat = Mat3::from_cols(from_r, from_g, from_b);
    let from_xyz_mat = Mat3::from_diagonal(from_rgb_mat.inverse() * from_w_xyz);
    from_rgb_mat * from_xyz_mat
}

pub fn conversion_matrix_from_to(from: &ColorspaceCoordinates, to: &ColorspaceCoordinates) -> Mat3 {
    let from_mat = conversion_matrix(from);
    let to_mat = conversion_matrix(to);
    to_mat.inverse() * from_mat
}

pub fn colorimetry_luminance(colorimetry: &Colorimetry) -> f32 {
    match colorimetry.tf {
        TransferFunction::Srgb => 80.0 / 10000.0,
        TransferFunction::St2084Pq => 1.0,
        // TransferFunction::St2084Pq => colorimetry
        //     .luminances
        //     .map_or(1.0, |luminance| luminance.max / 10000.0),
        TransferFunction::Gamma22 => 80.0 / 10000.0,
    }
}

pub fn luminance_scale_from_to(from: &Colorimetry, to: &Colorimetry) -> f32 {
    colorimetry_luminance(&from) / colorimetry_luminance(&to)
}

#[test]
fn test_conversion_matrix() {
    fn round_to_4(v: f32) -> i32 {
        (v * 10000f32 + 0.5f32) as i32
    }

    let srgb_to_xyz = Mat3::from_cols_array(&[
        0.4124, 0.2126, 0.0193, 0.3576, 0.7152, 0.1192, 0.1805, 0.0722, 0.9505,
    ]);

    let expected = srgb_to_xyz.to_cols_array().map(round_to_4);
    let actual = conversion_matrix(&get_coordinates(MatrixCoefficients::Srgb))
        .to_cols_array()
        .map(round_to_4);

    assert_eq!(expected, actual);

    let rec2020_to_xyz = Mat3::from_cols_array(&[
        0.6370, 0.2627, 0.0000, 0.1446, 0.6780, 0.0281, 0.1689, 0.0593, 1.0610,
    ]);

    let expected = rec2020_to_xyz.to_cols_array().map(round_to_4);
    let actual = conversion_matrix(&get_coordinates(MatrixCoefficients::Rec2020))
        .to_cols_array()
        .map(round_to_4);

    assert_eq!(expected, actual);

    let rec2020_to_srgb = Mat3::from_cols_array(&[
        1.6605, -0.1246, -0.0182, -0.5876, 1.1329, -0.1006, -0.0728, -0.0083, 1.1187,
    ]);

    let expected = rec2020_to_srgb.to_cols_array().map(round_to_4);
    let actual = conversion_matrix_from_to(
        &get_coordinates(MatrixCoefficients::Rec2020),
        &get_coordinates(MatrixCoefficients::Srgb),
    )
    .to_cols_array()
    .map(round_to_4);

    assert_eq!(expected, actual);

    let srgb_to_dcip3 = conversion_matrix_from_to(
        &get_coordinates(MatrixCoefficients::Srgb),
        &get_coordinates(MatrixCoefficients::DciP3),
    );

    let red_dcip3 = srgb_to_dcip3 * Vec3::new(1.0, 0.0, 0.0);

    assert!(red_dcip3[0] < 1.0);
    assert!(red_dcip3[1] > 0.0);
    assert!(red_dcip3[2] > 0.0);
}

#[derive(Debug)]
pub struct ColorManagedSurfaceRenderElement<R: NiriRenderer> {
    inner: WaylandSurfaceRenderElement<R>,
    program: GlesTexProgram,
    input: Colorimetry,
    output: Colorimetry,
    input_tf: TransferFunction,
    output_tf: TransferFunction,
    input_to_output: Mat3,
}

impl<R: NiriRenderer> From<ColorManagedSurfaceRenderElement<R>> for WaylandSurfaceRenderElement<R> {
    fn from(value: ColorManagedSurfaceRenderElement<R>) -> Self {
        value.inner
    }
}

impl<R: NiriRenderer> ColorManagedSurfaceRenderElement<R> {
    pub fn new(
        elem: WaylandSurfaceRenderElement<R>,
        renderer: &mut R,
        input: Colorimetry,
        output: Colorimetry,
    ) -> Self {
        let input_to_output = conversion_matrix_from_to(&input.coordinates, &output.coordinates)
            * luminance_scale_from_to(&input, &output);

        Self {
            inner: elem,
            program: Self::shader(renderer).unwrap(),
            input,
            output,
            input_tf: input.tf,
            output_tf: output.tf,
            input_to_output,
        }
    }

    pub fn inner(&self) -> &WaylandSurfaceRenderElement<R> {
        &self.inner
    }

    pub fn input_output(&self) -> (Colorimetry, Colorimetry) {
        (self.input, self.output)
    }

    pub fn shader(renderer: &mut R) -> Option<GlesTexProgram> {
        Shaders::get(renderer.as_gles_renderer())
            .color_manage
            .clone()
    }

    fn compute_uniforms(&self) -> Vec<Uniform<'static>> {
        vec![
            Uniform::new("input_tf", self.input_tf.to_uniform()),
            Uniform::new("output_tf", self.output_tf.to_uniform()),
            mat3_uniform("input_to_output", self.input_to_output),
        ]
    }
}

impl<R: NiriRenderer> Element for ColorManagedSurfaceRenderElement<R> {
    fn id(&self) -> &Id {
        self.inner.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.inner.geometry(scale)
    }

    fn transform(&self) -> Transform {
        self.inner.transform()
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.inner.src()
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.inner.damage_since(scale, commit)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        self.inner.opaque_regions(scale)
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl RenderElement<GlesRenderer> for ColorManagedSurfaceRenderElement<GlesRenderer> {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        frame.override_default_tex_program(self.program.clone(), self.compute_uniforms());
        RenderElement::<GlesRenderer>::draw(
            &self.inner,
            frame,
            src,
            dst,
            damage,
            opaque_regions,
            cache,
        )?;
        frame.clear_tex_program_override();
        Ok(())
    }

    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        // If scanout for things other than Wayland buffers is implemented, this will need to take
        // the target GPU into account.
        None
    }
}

impl<'render> RenderElement<TtyRenderer<'render>>
    for ColorManagedSurfaceRenderElement<TtyRenderer<'render>>
{
    fn draw(
        &self,
        frame: &mut TtyFrame<'render, '_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), TtyRendererError<'render>> {
        frame
            .as_gles_frame()
            .override_default_tex_program(self.program.clone(), self.compute_uniforms());
        RenderElement::draw(&self.inner, frame, src, dst, damage, opaque_regions, cache)?;
        frame.as_gles_frame().clear_tex_program_override();
        Ok(())
    }

    fn underlying_storage(
        &self,
        _renderer: &mut TtyRenderer<'render>,
    ) -> Option<UnderlyingStorage<'_>> {
        // If scanout for things other than Wayland buffers is implemented, this will need to take
        // the target GPU into account.
        None
    }
}
