# Power and battery

A laptop running Windows with a few apps open draws around 10–12 W, even when you're only waiting for an agent. Most of that goes to things you don't use while an agent works: the screen, the graphics card, background apps and services, and fast CPU cores that rarely sleep.

Lights Out mode keeps only what the agents need.

## What switches off

| Part | In Lights Out |
|---|---|
| Windows | fully shut down (Windows PCs) |
| Separate graphics card | no driver, powered off (NVIDIA); powered off when idle (AMD) |
| Bluetooth, sound, keyboard light | off |
| Screen | off whenever you want, and when the lid closes |
| USB, disk, PCIe chips | switch off by themselves when idle (keyboard and touchpad stay awake) |
| CPU turbo | off; the CPU prefers saving power |
| Fans | the firmware's quiet profile is selected; at these loads they usually stop |

Wi-Fi stays on, in power-save mode.

## How the CPU is used

Agents start on the 1–2 most frugal cores (on Intel, the low-power E-cores; where all cores are equal, the first two). The other cores sleep deeply.

There is no fixed watt limit. When work is waiting for a core (Lights Out reads the kernel's CPU pressure every few seconds), more cores join: first all but the fastest, then all. After a calm minute it steps back down. Several busy agents don't crawl, and the chip returns to deep sleep as soon as the work is done.

## What to expect

| | Draw | 71 Wh battery |
|---|---|---|
| Windows, light use | 10–12 W | about 6 hours |
| Lights Out, agents working, screen off | 3–5 W (target) | about 14–24 hours |
| Lights Out, idle, lid closed (measured) | 2.5 W | about 27 hours |

The measured line is an ASUS ROG Zephyrus G16 (2024). Your numbers depend on the laptop and on what the agents do. Claude Code mostly waits on the network, which costs almost nothing; builds and test runs cost the most.

On a Mac the gain is smaller, because macOS already idles efficiently: roughly 2–4 times longer with the screen off.

## Measuring

The battery level and the current draw are at the bottom left of the sidebar (on Windows and Linux). From a terminal:

```sh
lightsout power
```

For a fair number, let it run for 10–15 minutes with the screen off.
