// use std::io::Read;

// use moxcms::ColorProfile;
// use moxcms::ProfileClass;
// use moxcms::TransferCharacteristics;

// use crate::render_helpers::color_manage::Colorimetry;

// pub fn read_icc_file(path: &str) -> ColorProfile {
//     let mut file = std::fs::File::open(path).unwrap();
//     let mut buf = vec![];
//     file.read_to_end(&mut buf).unwrap();
//     let profile = ColorProfile::new_from_slice(&buf).unwrap();

//     profile
// }

// // pub fn profile_from_colorimetry(colorimetry: Colorimetry) -> ColorProfile {
// //     let mut profile = ColorProfile::default();
// //     profile.update_rgb_colorimetry_triplet(colorimetry.coordinates.w, red_xyz, green_xyz, blue_xyz);

// //     profile
// // }

// #[test]
// fn test_icc() {
//     let profile =
//         read_icc_file("/home/me/Downloads/MS-3DD3(MAG 321UP QD-OLED)_INF/MAG321UP OLED.icm");

//     println!("{:?}", profile);
//     assert_eq!(profile.profile_class, ProfileClass::Abstract);
// }
