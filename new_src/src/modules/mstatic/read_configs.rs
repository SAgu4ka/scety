use rayon::prelude::*;
use std::{fs, io::Result, path::PathBuf};

pub fn read_files(paths: Vec<PathBuf>) -> Result<Vec<Vec<u8>>> {
    paths.into_par_iter().map(|path| fs::read(path)).collect()
}
