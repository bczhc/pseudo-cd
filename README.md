Pseudo-CD
====

## Thoughts

Audio tracks are stored digitally on a Compact Disc (CD), and precisely
they're in PCM s16le format, which is known to be the format of standard
music CDs.

One compact disc can have multiple `session`s, and one session
can have multiple `track`s. For a music CD, multiple-track is used
and each track contains a song's PCM data.

If a music CD (here we explicitly name it, because a track can be either
audio or data type) is read on Linux, track data will not be mapped as a 
device file like `/dev/sr0`. And, copying the music data needs special
tools and methods. This process is called
[ripping](https://en.wikipedia.org/wiki/Ripping).

Modern DVDs (namely DVD-R) support multi-session, but there's
no multi-track like on CDs. If we consider each session on a DVD,
is equivalent to a track on a CD, then we can just "simulated"
a music CD. Just store PCM data as sessions. This is why I call
it a "pseudo CD". This project implements
a simple music player for such a DVD format.

I made this just for a little Rust programming practice.

## Format

| Session No. | Content                                         |
|-------------|-------------------------------------------------|
| 1           | Meta Info (title, creation time and track list) |
| 2           | Audio track #1                                  |
| 3           | Audio track #2                                  |
| 4           | Audio track #3                                  |
| 5           | ...                                             |

Meta info is a JSON string, in format:
```
{
  "title": <title>,
  "creation_time": <timestamp>,
  "list": [
    {
      "name": <name of audio track #1>,
      "session_no": 2
    },
    {
      "name": <name of audio track #2>,
      "session_no": 3
    },
    ...
  ]
}

```

## Authoring

1. Write the first session

   ```bash
   cdrskin -v -multi -data meta.json
   ```
2. Write audio tracks
   
   For each audio file, first convert it to PCM data:
   ```bash
   ffmpeg -i {} -c:a pcm_s16le -ar 44100 -ac 2 -f s16le output
   ```
   Then burn it:
   ```bash
   cdrskin -v -multi -data output
   ```

Here's a way to do 2. in batch:
```bash
cat song-list | xargs -d\\n -n1 bash -c 'cdrskin -v -multi -data "$1"' --
```

Finally, if using `cdrskin -minfo` or `wodim -minfo` to see the DVD
content, it's expected to get something like this:

```
Track  Sess Type   Start Addr End Addr   Size
==============================================
    1     1 Data   0          65263      65264
    2     2 Data   93952      112271     18320
    3     3 Data   118432     138735     20304
    4     4 Data   144896     178799     33904
    5     5 Data   184960     200063     15104
    6     6 Data   206224     224815     18592
...
```

where Sess 1 is the meta info, and audio tracks start from
Sess 2.

## CLI Options

<pre>
Usage: pseudo-cd-player [OPTIONS] [DRIVE]

Arguments:
[DRIVE]
Path of the disc drive (like /dev/sr0 on Linux) TODO: on platforms other than *nix?

          [default: /dev/sr0]

Options:
-m, --meta-info-track <META_INFO_TRACK>
Number (starts from one) of the track that stores meta info of this "Pseudo-CD" authoring

          By default, the first track is picked.
          
          [default: 1]

      --no-meta
          On true, assume all tracks are PCM data

      --minfo-program <MINFO_PROGRAM>
          Program to fetch optical medium info
          
          [default: cdrskin]
          [possible values: cdrskin, cdrecord, wodim]

-l, --log-file <LOG_FILE>
Program log will output to this if present

-h, --help
Print help (see a summary with '-h')

</pre>

## Screenshot

<img width="100%" alt="image" src="https://github.com/user-attachments/assets/a6317df1-65ae-4039-b865-7ed2d6bae724">

## TUI Key Bindings

```
Space: Play/Pause
n: Next
p: Previous
j, ArrowDown: Selection move up
k, ArrowUp: Selection move down
g, Home: Move selection to the first
G, End: Move selection to the last
h, ArrowLeft: Seek backwards 5 seconds
l, ArrowRight: Seek forward 5 seconds
Enter: Play the selection
,: Volume down
.: Volume up
```