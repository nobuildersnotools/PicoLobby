package dev.quozul;

import com.sun.jna.Pointer;
import java.nio.file.Path;

public class PicoLobbyRunner implements Runnable {

    private final Path configurationPath;
    private final Standalone.RustLib lib;
    private volatile Pointer cancellation_token;

    public PicoLobbyRunner(Path configurationPath) throws Exception {
        this.lib = Standalone.loadLib();
        this.configurationPath = configurationPath;
    }

    @Override
    public void run() {
        String[] args = {
                "pico_lobby_java_wrapper",
                "--config",
                configurationPath.toString()
        };

        cancellation_token = lib.get_cancellation_token();
        try {
            lib.start_app(cancellation_token, args.length, args);
        } finally {
            lib.cleanup_token(cancellation_token);
            cancellation_token = null;
        }
    }

    public void stop() {
        Pointer token = cancellation_token;
        if (token != null) {
            lib.stop_app(token);
        }
    }
}
