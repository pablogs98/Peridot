use std::env::args;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use log::info;
use std::ffi::CString;
use std::os::fd::RawFd;
use libc::{open, O_CREAT, O_RDWR, O_TRUNC, S_IRUSR, S_IWUSR};

fn main() {
    // get time
    let args = args().collect::<Vec<String>>();
    if args.len() != 3 {
        println!("Usage: imagenet-preprocessing <use_geds:true|false> <num_images>");
        info!("Usage: imagenet-preprocessing <use_geds:true|false> <num_images>");
        return;
    }
    println!("Starting imagenet-preprocessing");
    let use_geds = args[1].parse::<bool>().unwrap();
    let num_images = args[2].parse::<usize>().unwrap();

    let image_paths = [
        "/home/ubuntu/Peridot/crates/imagenet-preprocessing/resources/tabby.png",
        "/home/ubuntu/Peridot/crates/imagenet-preprocessing/resources/banana.jpg",
        "/home/ubuntu/Peridot/crates/imagenet-preprocessing/resources/cougar.jpg",
    ];
    let image_paths = image_paths
        .iter()
        .map(|p| PathBuf::from(p))
        .collect::<Vec<PathBuf>>();
    let start = std::time::Instant::now();
    let images = read_images(&image_paths, num_images).unwrap();
    let elapsed = start.elapsed();
    println!("Read images in {} ms", elapsed.as_millis());
    let start = std::time::Instant::now();
    let batch_tensors = preprocess(&images);
    let elapsed = start.elapsed();
    println!("Preprocessed images in {} ms", elapsed.as_millis());
    let start = std::time::Instant::now();
    // Write batch_tensors to files
    for (i, tensor) in batch_tensors.iter().enumerate() {
        let file_name;
        if use_geds {
            file_name = format!("/home/ubuntu/Peridot/crates/imagenet-preprocessing/geds://geds-default/tensor_{}.bin", i);
            write_tensor_to_file(&file_name, tensor).unwrap();
            continue;
        } else {
            file_name = format!("tensor_{}.bin", i);
        }
        write_tensor_to_file(&file_name, tensor).unwrap();
    }
    let elapsed = start.elapsed();
    println!("Wrote tensors to files in {} ms", elapsed.as_millis());
}

fn write_tensor_to_file(file_name: &str, tensor: &[u8]) -> std::io::Result<()> {
    println!("Trying to open file {}", file_name);
    {
        fs::write(file_name, tensor)?;
        println!("Opened {}", file_name);
    }
    Ok(())
}


fn read_images(image_paths: &[PathBuf], num_images: usize) -> Result<Vec<Vec<u8>>, ()> {
    let mut images = Vec::new();
    let mut i = 0;
    while i < num_images {
        let image = fs::read(&image_paths[i % 3]).unwrap();
        images.push(image);
        i = i + 1;
    }
    Ok(images)
}

fn preprocess(images: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut processed_images = Vec::new();
    for image in images {
        let processed_image = preprocess_one(
            image,
            224,
            224,
            &[0.485, 0.456, 0.406],
            &[0.229, 0.224, 0.225],
        );
        processed_images.push(processed_image);
    }
    processed_images
}

fn preprocess_one(image: &[u8], height: u32, width: u32, _mean: &[f32], _std: &[f32]) -> Vec<u8> {
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
