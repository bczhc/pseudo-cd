Pseudo-CD
====

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
a music CD. Just store PCM data as sessions. This project implements
a simple music player for such a DVD format.


