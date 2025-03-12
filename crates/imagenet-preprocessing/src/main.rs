use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let image_paths = ["fixture/two/tabby.png","fixture/two/banana.jpg", "fixture/two/cougar.jpg"];
    let image_paths = image_paths.iter().map(|p| PathBuf::from(p)).collect::<Vec<PathBuf>>();
    let images  = read_images(&image_paths).unwrap();
    let batch_tensors = preprocess(&images);
    // Write batch_tensors to files
    for (i, tensor) in batch_tensors.iter().enumerate() {
        let file_name = format!("tensor_{}.bin", i);
        write_tensor_to_file(&file_name, tensor).unwrap();
    }
}

fn write_tensor_to_file(file_name: &str, tensor: &[u8]) -> std::io::Result<()> {
    let mut file = File::create(file_name)?;
    file.write_all(tensor)?;
    Ok(())
}

fn read_images(image_paths: &[PathBuf]) -> Result<Vec<Vec<u8>>, ()> {
    let mut images = Vec::new();
    for image_path in image_paths {
        let image = fs::read(image_path).unwrap();
        images.push(image);
    }
    Ok(images)
}

fn preprocess(images: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut processed_images = Vec::new();
    for image in images {
        let processed_image = preprocess_one(image, 224, 224, &[0.485, 0.456, 0.406], &[0.229, 0.224, 0.225]);
        processed_images.push(processed_image);
    }
    processed_images
}

fn preprocess_one(image: &[u8], height: u32, width: u32, _mean: &[f32], _std: &[f32]) -> Vec<u8> {
    println!("Preprocessing image to size {}x{}...", width, height);
    //Get size of image
    println!("Image len: {}", image.len());
    let img = image::load_from_memory(&image).unwrap().to_rgb8();
    let resized =
        image::imageops::resize(&img, height, width, ::image::imageops::FilterType::Triangle);

    let mut flat_img: Vec<f32> = Vec::new();
    for rgb in resized.pixels() {
        flat_img.push((rgb[0] as f32 / 255. - 0.485) / 0.229);
        flat_img.push((rgb[1] as f32 / 255. - 0.456) / 0.224);
        flat_img.push((rgb[2] as f32 / 255. - 0.406) / 0.225);
    }
    let bytes_required = flat_img.len() * 4;
    let mut u8_f32_arr: Vec<u8> = vec![0; bytes_required];

    for c in 0..3 {
        for i in 0..(flat_img.len() / 3) {
            let u8_f32: f32 = flat_img[i * 3 + c];
            let u8_bytes = u8_f32.to_ne_bytes();

            for j in 0..4 {
                u8_f32_arr[((flat_img.len() / 3 * c + i) * 4) + j] = u8_bytes[j];
            }
        }
    }
    u8_f32_arr
}
