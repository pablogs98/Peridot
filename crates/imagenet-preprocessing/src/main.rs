use std::env::args;
use std::{fs, thread};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use log::info;
use std::ffi::CString;
use std::os::fd::RawFd;
use std::time::Duration;
use libc::{open, O_CREAT, O_RDWR, O_TRUNC, S_IRUSR, S_IWUSR};

fn main() {
    // get time
    let args = args().collect::<Vec<String>>();
    if args.len() != 2 {
        println!("Usage: imagenet-preprocessing <use_geds:true|false> <num_images>");
        info!("Usage: imagenet-preprocessing <use_geds:true|false> <num_images>");
        return;
    }
    println!("Starting imagenet-preprocessing");
    let use_geds = args[0].parse::<bool>().unwrap();
    let num_images = args[1].parse::<usize>().unwrap();

    let mut image_paths = vec![];
    for image in fs::read_dir("./resources").unwrap() {
        if image_paths.len() >= num_images {
            break;
        }
        let image = image.unwrap();
        if image.path().extension().is_some_and(|ext| ext == "jpeg" || ext == "png" || ext == "jpg") {
            //println!("Found image: {}", image.path().display());
            image_paths.push(image.path().to_str().unwrap().to_string());
        }
    }

    let image_paths = image_paths
        .iter()
        .map(|p| PathBuf::from(p))
        .collect::<Vec<PathBuf>>();
    let start = std::time::Instant::now();
    let images = read_images(&image_paths, num_images).unwrap();
    let elapsed = start.elapsed();
    println!("Read images in {} ms", elapsed.as_millis());
    let start = std::time::Instant::now();
    let batch_tensors = preprocess(&images, use_geds);
    let elapsed = start.elapsed();
    println!("Preprocessed images in {} ms", elapsed.as_millis());
}

fn write_tensor_to_file(file_name: &str, tensor: &[u8]) -> std::io::Result<()> {
    println!("Trying to open file {}", file_name);
    let mut attempts = 0;
    loop {
        match fs::write(file_name, tensor) {
            Ok(_) => {
                println!("Opened {}", file_name);
                return Ok(());
            }
            Err(e) if attempts < 5 => {
                println!("Retrying write (attempt {}): {}", attempts + 1, e);
                attempts += 1;
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e),
        }
    }
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

fn preprocess(images: &[Vec<u8>], use_geds: bool) -> Vec<Vec<u8>> {
    let mut processed_images = Vec::new();
    let mut i = 0;
    for image in images {
        let processed_image = preprocess_one(
            image,
            224,
            224,
            &[0.485, 0.456, 0.406],
            &[0.229, 0.224, 0.225],
        );
        let file_name = if use_geds {
            format!("s3://pgimeno-data/tensors/tensor_{}.bin", i)
        } else {
            format!("./tensors/tensors/tensor_{}.bin", i)
        };
        
        write_tensor_to_file(&file_name, &processed_image).unwrap();

        processed_images.push(processed_image);
        i += 1;
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