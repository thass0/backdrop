#/usr/bin/env bash

set -x

while getopts i:a:o: flag
do
  case "${flag}" in
    i) image=${OPTARG};;
    a) audio=${OPTARG};;
    o) output=${OPTARG};;
  esac
done

# Get length of the audio file.
length="$(ffprobe -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 "$audio" 2>/dev/null)";

ffmpeg -r 1 -loop 1 -i $image -i $audio \
  -acodec copy \
  -vcodec libx264 -tune stillimage -preset ultrafast \
  -ss 0 -t $length \
  $output -y

xdg-open $output
  
