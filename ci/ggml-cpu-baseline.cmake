# Pin ggml/whisper.cpp to a portable x86-64 CPU baseline for CI builds.
#
# GGML_NATIVE (ggml's default) compiles for the CPU of the machine doing the
# build. GitHub runners are a hardware lottery — some have AVX-512, and an
# installer built there dies with 0xc000001d (illegal instruction) on any
# user CPU without it (e.g. AMD Zen 2 laptops). AVX2+FMA+F16C is every
# x86-64 CPU since ~2013 and is what upstream whisper.cpp ships in its own
# release binaries.
#
# Injected via the CMAKE_TOOLCHAIN_FILE env var in .github/workflows.
set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
set(GGML_AVX2 ON CACHE BOOL "" FORCE)
set(GGML_FMA ON CACHE BOOL "" FORCE)
set(GGML_F16C ON CACHE BOOL "" FORCE)
set(GGML_AVX512 OFF CACHE BOOL "" FORCE)
