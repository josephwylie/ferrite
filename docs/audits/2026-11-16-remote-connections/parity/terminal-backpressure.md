# Remote terminal input backpressure

A remote terminal retains eight queued input frames and at most one pending frame. When the PTY stops consuming input, the gateway waits for queue capacity while continuing to forward output and check process exit and session revocation.

If one pending frame cannot enter the queue for five continuous seconds, the gateway closes the terminal and reaps its PTY child. This also releases a disconnected client whose close frame is behind blocked input. Error delivery and WebSocket close each have a two-second bound. Consequently, a sustained paste into a program that refuses terminal input is terminated; ordinary idle terminals without pending input remain connected.

The network regressions exercise a 512 KiB paste in 128 frames through a temporarily blocked PTY, verify continued output and exact received bytes, and disconnect a permanently blocked PTY to check child reaping and connection-slot release. The parent verification run records their result.
