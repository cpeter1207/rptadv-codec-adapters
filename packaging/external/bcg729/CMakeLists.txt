cmake_minimum_required(VERSION 3.5)
project(rptadv_bcg729_gpl2 C)
set(ENABLE_SHARED ON CACHE BOOL "" FORCE)
set(ENABLE_STATIC OFF CACHE BOOL "" FORCE)
set(ENABLE_TESTS OFF CACHE BOOL "" FORCE)
add_subdirectory("${UPSTREAM_SOURCE_DIR}" upstream)
# Never resolve a current GPL3 libbcg729 under the reviewed GPL2 dependency name.
set_target_properties(bcg729 PROPERTIES OUTPUT_NAME rptadv_bcg729_gpl2 VERSION 0 SOVERSION 0)
