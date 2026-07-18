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
#
# Compile ggml/whisper with clang-cl (choco LLVM), not the runner's MSVC.
# The runners' default is a preview MSVC v18 whose ggml output decoded
# 5-20x slower than local v143 builds and died silently mid-decode on user
# machines. Two earlier pins failed: `-T ClangCL` breaks on the preview VS,
# and CMAKE_GENERATOR_TOOLSET set HERE is silently ignored — a toolchain
# file is read after the generator toolset is fixed, so v0.1.0 still
# shipped preview-MSVC kernels (proven by the exe's Rich header; 30 s
# decodes on Zen 2). Pinning the compiler itself works with any generator;
# CI uses Ninja (CMAKE_GENERATOR env) so no VS toolset selection happens at
# all. clang-cl finds the MSVC headers/libs + Windows SDK by itself, and
# upstream whisper.cpp/llama.cpp ship clang-built Windows releases too.
find_program(OPENFLOW_CLANG_CL clang-cl HINTS "C:/Program Files/LLVM/bin")
if(NOT OPENFLOW_CLANG_CL)
    message(FATAL_ERROR "clang-cl not found - install LLVM (choco install llvm)")
endif()
set(CMAKE_C_COMPILER "${OPENFLOW_CLANG_CL}")
set(CMAKE_CXX_COMPILER "${OPENFLOW_CLANG_CL}")

# Post-project() include: verifies the pin took effect (fail the build
# rather than ever ship slow preview-MSVC kernels again) and adds the
# flags clang-cl needs that cl implies (/EHsc, FMA/F16C target features).
# Flags must live there, not in *_FLAGS_INIT: the cmake-rs crate passes an
# explicit CMAKE_C(XX)_FLAGS that overrides the INIT variables.
set(CMAKE_PROJECT_INCLUDE "${CMAKE_CURRENT_LIST_DIR}/assert-clang.cmake")

set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
set(GGML_AVX2 ON CACHE BOOL "" FORCE)
set(GGML_FMA ON CACHE BOOL "" FORCE)
set(GGML_F16C ON CACHE BOOL "" FORCE)
set(GGML_AVX512 OFF CACHE BOOL "" FORCE)
