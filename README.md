# Backdrop - seamlessly create background videos

Backdrop is a web app which can be used to combine pieces of
music and images into videos, where the music plays with the
image in the background.

Its purpose is to decrease the fiction for musicians of manually
rendering such videos to publish their music on (e.g.) YouTube.

It is still in **extremely early development** and doesn't do much
except for what was just described.

## Usage

Landing on the app's home page, you can upload both an MP3 file and a JPG file.
Together, both files cannot be larger than 4 MB.

After selecting two files, hit the *submit* button to upload them and kick of
the rendering process.

You'll be redirected to a download page where you can wait for the render to finish.
Once the video is ready, the *download* button will light up to let you download
the result.

Now you should have an MP4 of the given image with your music playing in the background!

## Installation

Running the following command in you terminal will install and optionally
run an instance of backdrop:

```bash
bash -c "$(curl -sLo- https://raw.githubusercontent.com/d4ckard/backdrop/main/scripts/install.sh)"
```

Alternatively, you can manually install and run your own instance following the steps below.

### 1. Prerequisites

To build the app you need to have an up-to-date version of
[**Docker**](https://docs.docker.com/get-docker/) installed.

*Optionally*, you can also install [bunyan-rs](https://crates.io/crates/bunyan)
to get pretty printed logs. This will however require you to install
the [Rust toolchain](https://www.rust-lang.org/tools/install) too.

### 2. Running the app in Docker

To start off you'll need to have docker running in the background.

```bash
# Start Docker to run in the background
$ systemctl --user start docker-desktop
```

To run the app itself, you can use the bash script `run_local.sh` located in the
`scripts` folder (You have to run the script from the root of this repository so
Docker can find the Dockerfile).

```bash
# Make the script executable.
$ chmod +x scripts/run_local.sh

# Run it!
$ ./scripts/run_local.sh
```

Don't worry: running the script might take a while.
Waiting for Docker to build the image the first time around might take
a few minutes even.

After the build is finished and the app is running, you should be able
to find it in your browser under [http://localhost:8000/](http://localhost:8000/).

Otherwise, if you run into a problem building the image please
file an [issue](https://github.com/d4ckard/backdrop/issues).
