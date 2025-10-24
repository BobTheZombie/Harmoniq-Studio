# Udev permissions for Harmoniq Studio

Harmoniq Studio needs realtime access to MIDI controllers and low-latency audio
interfaces. For Flatpak builds, we ship permissive defaults via the `--device`
finish arguments. On traditional distributions you can replicate the behaviour
by installing the following udev rules to `/etc/udev/rules.d/99-harmoniq.rules`:

```udev
SUBSYSTEM=="usb", ENV{ID_AUDIO}=="1", TAG+="uaccess"
KERNEL=="snd-seq", MODE="0666"
KERNEL=="rtc0", GROUP="audio"
```

After adding the rules, reload udev and replug your devices:

```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
```

Members of the `audio` group automatically receive realtime scheduling rights.
When running inside Flatpak these permissions are mediated by the portal layer
and no host-side changes are required beyond adding your user to the `audio`
group if it is not already present.
