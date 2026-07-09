package org.rockbox.ffi

import java.io.File

/** Locate the repo root + bundled test fixture, walking up from the cwd. */
internal object Fixtures {
    val repoRoot: File by lazy {
        var dir: File? = File(System.getProperty("user.dir")).absoluteFile
        while (dir != null) {
            if (File(dir, "crates/rocksky/fixtures").isDirectory) return@lazy dir
            dir = dir.parentFile
        }
        throw RockboxException("could not find repo root (crates/rocksky/fixtures) from cwd")
    }

    val sample: String
        get() = File(repoRoot, "crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a").path
}
