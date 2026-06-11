package dev.quozul;

import java.nio.file.Path;

public class PicoLobbyRunner implements Runnable {

    private final Path configurationPath;

    public PicoLobbyRunner(Path configurationPath) {
        this.configurationPath = configurationPath;
    }

    @Override
    public void run() {
        String[] args = {"--config", configurationPath.toString()};
        Standalone.main(args);
    }

    public void stop() {
    }
}
