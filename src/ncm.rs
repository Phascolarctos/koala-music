// use ncmmiao::Ncmfile;
// use std::path::Path;
// use std::sync::atomic::AtomicBool;
// use std::sync::Arc;

// let mut ncm = Ncmfile::new("song.ncm")?;
// let (tx, _rx) = crossbeam_channel::unbounded::<ncmmiao::Signal>();
// let cancel = Arc::new(AtomicBool::new(false));
// ncm.dump_to_file(Path::new("output/"), Some(tx), false, cancel)?;