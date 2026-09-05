import com.vanniktech.maven.publish.SonatypeHost
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    kotlin("jvm") version "2.3.20"
    application
    id("com.vanniktech.maven.publish") version "0.30.0"
}

group = "io.github.tsirysndr"
version = providers.gradleProperty("libVersion").getOrElse("0.7.0")

repositories { mavenCentral() }

dependencies {
    implementation("org.json:json:20250107")
}

// Compiled and run on the Temurin 25 JDK pinned by mise.toml; the Foreign
// Function & Memory API (JDK 22+) resolves from that JDK's classpath. Bytecode
// targets 21 (an LTS floor) — keep the Java + Kotlin tasks in lockstep.
java {
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_21)
    }
}

// FFM's restricted native methods require an explicit grant on JDK 25 (else a
// warning is printed on first call and a hard error in a future release).
val nativeAccess = listOf("--enable-native-access=ALL-UNNAMED")

application {
    mainClass.set("org.rockbox.ffi.SmokeKt")
    applicationDefaultJvmArgs = nativeAccess
}

tasks.register<JavaExec>("smoke") {
    group = "application"
    description = "Run the end-to-end smoke test"
    mainClass.set("org.rockbox.ffi.SmokeKt")
    classpath = sourceSets["main"].runtimeClasspath
    jvmArgs = nativeAccess
}

tasks.register<JavaExec>("play") {
    group = "application"
    description = "Play an audio file (pass -Pfile=/path/to/audio)"
    mainClass.set("org.rockbox.ffi.PlayKt")
    classpath = sourceSets["main"].runtimeClasspath
    jvmArgs = nativeAccess
    if (project.hasProperty("file")) args(project.property("file").toString())
}

// ---- publishing: Maven Central (Sonatype Central Portal) --------------
// The jar bundles the prebuilt librockbox_ffi for every desktop OS/arch under
// src/main/resources/native/<target>/, plus the Android arm64-v8a + x86_64 .so
// under src/main/resources/lib/<abi>/ (which AGP extracts into a consuming
// app's APK). Both are staged by scripts/fetch-libs.sh from the GitHub release,
// so consumers need no Rust toolchain and no separate lib.
//
// Credentials (in ~/.gradle/gradle.properties or ORG_GRADLE_PROJECT_* env):
//   mavenCentralUsername / mavenCentralPassword   (a Central Portal token)
//   signingInMemoryKey / signingInMemoryKeyPassword  (ASCII-armored GPG key)
// Publish:  ./gradlew publishToMavenCentral   (add --no-configuration-cache)
mavenPublishing {
    publishToMavenCentral(SonatypeHost.CENTRAL_PORTAL, automaticRelease = true)
    signAllPublications()

    coordinates(group.toString(), "rockbox-ffi", version.toString())

    pom {
        name.set("rockbox-ffi")
        description.set(
            "Kotlin bindings for the Rockbox DSP, metadata, and playback engine " +
                "(Java FFM over the shared rockbox-ffi C ABI).",
        )
        inceptionYear.set("2026")
        url.set("https://github.com/tsirysndr/rockboxd")
        licenses {
            license {
                name.set("GPL-2.0-or-later")
                url.set("https://www.gnu.org/licenses/old-licenses/gpl-2.0.html")
                distribution.set("repo")
            }
        }
        developers {
            developer {
                id.set("tsirysndr")
                name.set("Tsiry Sandratraina")
                url.set("https://github.com/tsirysndr")
            }
        }
        scm {
            url.set("https://github.com/tsirysndr/rockboxd")
            connection.set("scm:git:git://github.com/tsirysndr/rockboxd.git")
            developerConnection.set("scm:git:ssh://git@github.com/tsirysndr/rockboxd.git")
        }
    }
}
