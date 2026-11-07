# An uncommissioned Always-On Device advertises until it is commissioned

Matter's default commissioning window closes after 15 minutes and needs a button press to reopen. An uncommissioned Always-On Device instead advertises over BLE until it is commissioned, because these are personal Devices in your own home, and an expired window would just mean walking to the Device. Sleepy Devices revisit this when they become real, since constant BLE advertising drains a battery.
