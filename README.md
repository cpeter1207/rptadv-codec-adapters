# RPT Advanced codec providers

These shared libraries encode and decode mono audio for RPT Advanced links.
The controller and protocol libraries load them through codec ABI 1. They do
not open radios or network connections.

Install the runtime packages required by your controller release from the
[signed APT repository](https://cpeter1207.github.io/USBRadioPlus/):

```sh
sudo apt update
sudo apt install rpt-advanced
```

The controller package selects its required providers. Development headers are
in `librptadv-codec-dev`; they are not needed to operate a node.

| Runtime package | Formats |
| --- | --- |
| `librptadv-codec-g7111` | ULAW, ALAW |
| `librptadv-codec-pcm1` | SLIN, SLIN16 |
| `librptadv-codec-gsm1` | GSM |
| `librptadv-codec-opus1` | Opus |
| `librptadv-codec-speex1` | Speex, Speex16 |
| `librptadv-codec-spandsp1` | G.726 AAL2, G.726, G.722, LPC10 |
| `librptadv-codec-bcg7291` | G.729 |
| `librptadv-codec-ilbc1` | iLBC |
| `librptadv-codec-dialogic1` | Dialogic ADPCM |
| `librptadv-codec-ffmpeg1` | G.723.1 |

G.723.1 sends 6.3 kb/s and receives 5.3/6.3 kb/s. The iLBC provider supports 20 ms
and 30 ms modes; IAX connections use 30 ms. Opus uses 48 kHz mono audio. Codec-native
loss concealment stays enabled where the decoder provides it.

Siren7, Siren14, and G.719 are not offered. Read the controller's link guide for
its codec selection and packet settings.

Developer references: [quality checks](QUALITY.md) and
[source and license evidence](dependencies/review.md).
