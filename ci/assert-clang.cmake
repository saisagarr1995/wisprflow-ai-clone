# Injected via CMAKE_PROJECT_INCLUDE from ggml-cpu-baseline.cmake: runs
# after project() has detected the compiler — the only point where the
# toolchain pin can be VERIFIED (a toolchain file runs too early to know
# the compiler) and where flags survive the cmake-rs crate, which passes
# an explicit CMAKE_C(XX)_FLAGS that overrides any *_FLAGS_INIT.
#
# Tripwire for the CI-binary saga: v0.1.0 shipped whisper kernels compiled
# by the runner's preview MSVC v18 — 5-20x slower decodes + silent
# mid-decode death on user CPUs — because the earlier toolset pin was
# silently ignored. Fail the build loudly instead of shipping a slow binary.
# Per-language, and only when detection has produced an ID: this file is
# also pulled into CMake's own compiler-test mini-projects, where the
# other language's ID is empty — that must not trip the assert.
foreach(lang C CXX)
    if(NOT "${CMAKE_${lang}_COMPILER_ID}" STREQUAL ""
       AND NOT CMAKE_${lang}_COMPILER_ID STREQUAL "Clang")
        message(FATAL_ERROR "ggml/whisper must be built with clang-cl, got "
            "${CMAKE_${lang}_COMPILER_ID} ${CMAKE_${lang}_COMPILER_VERSION} "
            "(${CMAKE_${lang}_COMPILER}). The compiler pin in "
            "ggml-cpu-baseline.cmake did not take effect - do NOT ship "
            "this binary.")
    endif()
endforeach()
message(STATUS "openflow: ggml compiler: C='${CMAKE_C_COMPILER_ID} "
    "${CMAKE_C_COMPILER_VERSION}' CXX='${CMAKE_CXX_COMPILER_ID} "
    "${CMAKE_CXX_COMPILER_VERSION}'")

# clang-cl needs what MSVC's cl forgives/implies:
#  - /EHsc: cl compiles try/catch without it (warning C4530); clang-cl
#    hard-errors ("cannot use 'try' with exceptions disabled").
#  - -mfma -mf16c: ggml's MSVC branch passes /arch:AVX2 and hand-defines
#    the GGML_FMA/GGML_F16C macros, but under clang /arch:AVX2 does not
#    enable the fma/f16c TARGET FEATURES, so the fast kernels would be
#    silently dropped (or fail to compile). -mavx2 is implied by
#    /arch:AVX2 but pinned here too for belt and braces.
add_compile_options("$<$<COMPILE_LANGUAGE:CXX>:/EHsc>" -mavx2 -mfma -mf16c)
