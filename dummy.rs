/*
This file is used for optimizing incremental builds in dockerfile. It does not matter for whole build,
but if small changes are made then it's good to use snapshot of precompiled dependencies instead of rebuilding them every time
 */

fn main() {
    println!("Dummy file, if you see this output that means docker build went wrong");
    eprintln!("Dummy file, if you see this output that means docker build went wrong");
}
