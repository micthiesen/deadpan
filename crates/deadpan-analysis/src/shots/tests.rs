use super::*;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 36;

/// A picture filled by `color(x, y)`.
fn picture(color: impl Fn(u32, u32) -> [u8; 3]) -> PictureSignature {
    let rgba: Vec<u8> = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let [r, g, b] = color(x, y);
            [r, g, b, 255]
        })
        .collect();
    PictureSignature::from_rgba(&rgba, WIDTH, HEIGHT, WIDTH as usize * 4).unwrap()
}

/// A warm scene with a soft horizontal gradient, shifted by `offset` pixels.
fn scene_a(offset: u32) -> PictureSignature {
    picture(|x, _| {
        let shade = ((x + offset) * 3 % 192) as u8;
        [200, 120 + shade / 4, 40]
    })
}

/// A dark blue scene with a vertical gradient.
fn scene_b() -> PictureSignature {
    picture(|_, y| [10, 20, 90 + (y * 4) as u8])
}

#[test]
fn a_cut_between_scenes_is_one_boundary() {
    let signatures: Vec<_> = (0..20)
        .map(|index| {
            if index < 12 {
                scene_a(index)
            } else {
                scene_b()
            }
        })
        .collect();
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    assert_eq!(analysis.boundaries(), [12]);
    let [cell, histogram, skip] = analysis.changes()[12];
    assert!(
        cell >= 24 && histogram >= 48 && skip >= 24,
        "{cell} {histogram} {skip}"
    );
}

#[test]
fn a_slow_pan_within_one_scene_is_not_a_cut() {
    let signatures: Vec<_> = (0..30).map(|index| scene_a(index * 2)).collect();
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    assert!(analysis.boundaries().is_empty(), "{:?}", analysis.changes());
}

#[test]
fn a_flash_frame_returning_to_its_scene_is_not_a_cut() {
    let mut signatures: Vec<_> = (0..20).map(scene_a).collect();
    signatures[10] = picture(|_, _| [255, 255, 255]);
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    assert!(analysis.boundaries().is_empty(), "{:?}", analysis.changes());
}

#[test]
fn cuts_closer_than_six_pictures_keep_the_first() {
    let signatures: Vec<_> = (0..24)
        .map(|index| match index {
            0..10 => scene_a(index),
            10..13 => scene_b(),
            _ => picture(|_, _| [255, 255, 255]),
        })
        .collect();
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    assert_eq!(analysis.boundaries(), [10]);
}

#[test]
fn identical_pictures_do_not_change() {
    assert_eq!(scene_b().change(&scene_b(), Some(&scene_b())), [0, 0, 0]);
    let black = picture(|_, _| [0, 0, 0]);
    let white = picture(|_, _| [255, 255, 255]);
    assert_eq!(black.change(&white, None)[1], 255, "disjoint histograms");
    assert_eq!(
        black.change(&white, Some(&white))[2],
        0,
        "skip compares with the picture two before"
    );
}

#[test]
fn stored_changes_and_buffers_are_validated() {
    assert!(ShotAnalysis::new(vec![[1, 0, 0]]).is_err());
    assert!(ShotAnalysis::new(vec![]).unwrap().boundaries().is_empty());
    assert!(PictureSignature::from_rgba(&[0; 16], 2, 2, 8).is_err());
    assert!(PictureSignature::from_rgba(&[0; 64 * 36 * 4 - 1], 64, 36, 256).is_err());
}

#[test]
fn padded_rows_are_read_by_stride() {
    let tight = scene_b();
    let stride = WIDTH as usize * 4 + 12;
    let mut padded = vec![7_u8; stride * HEIGHT as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let offset = y as usize * stride + x as usize * 4;
            padded[offset..offset + 4].copy_from_slice(&[10, 20, 90 + (y * 4) as u8, 255]);
        }
    }
    let signature = PictureSignature::from_rgba(&padded, WIDTH, HEIGHT, stride).unwrap();
    assert_eq!(signature, tight);
}
