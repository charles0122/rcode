import com.android.build.gradle.tasks.ExternalNativeBuildTask

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

val stagedRustJniLibs = layout.buildDirectory.dir("generated/rustJniLibs")
val stageRustLibraries by tasks.registering {
    val repoRoot = projectDir.resolve("../../..").canonicalFile
    val sourceDir = repoRoot.resolve("dist/core/android")
    val outputDir = stagedRustJniLibs.get().asFile
    inputs.files(
        sourceDir.resolve("arm64-v8a/librcode.so"),
        sourceDir.resolve("x86_64/librcode.so"),
    )
    outputs.dir(outputDir)
    doLast {
        val expected = listOf("arm64-v8a", "x86_64")
        expected.forEach { abi ->
            val source = sourceDir.resolve("$abi/librcode.so")
            check(source.isFile) {
                "Missing $source; run scripts/build-mobile-core.sh from the repository root first"
            }
            val destination = outputDir.resolve("$abi/librcode.so")
            destination.parentFile.mkdirs()
            source.copyTo(destination, overwrite = true)
        }
    }
}

android {
    namespace = "com.rcode.scanner"
    compileSdk = 36

    defaultConfig {
        minSdk = 24
        ndkVersion = "28.2.13676358"
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
        consumerProguardFiles("consumer-rules.pro")
        externalNativeBuild {
            cmake {
                cppFlags += "-std=c++17"
                arguments += "-DRCODE_LIBRARY_DIR=${stagedRustJniLibs.get().asFile.absolutePath}"
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    sourceSets["main"].jniLibs.srcDir(stagedRustJniLibs)
    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }
    testOptions { unitTests.isReturnDefaultValues = true }
}

tasks.named("preBuild").configure { dependsOn(stageRustLibraries) }
tasks.withType<ExternalNativeBuildTask>().configureEach { dependsOn(stageRustLibraries) }

dependencies {
    val cameraX = "1.6.2"
    implementation("androidx.camera:camera-camera2:$cameraX")
    implementation("androidx.camera:camera-lifecycle:$cameraX")
    api("androidx.camera:camera-view:$cameraX")
    api("androidx.lifecycle:lifecycle-runtime-ktx:2.10.0")

    testImplementation("junit:junit:4.13.2")
}
