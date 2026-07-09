import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    kotlin("jvm") version "2.3.20"
    application
}

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
