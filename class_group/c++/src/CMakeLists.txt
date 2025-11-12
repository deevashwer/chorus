# set (BICYCL_MAIN_HEADER ${CMAKE_CURRENT_SOURCE_DIR}/bicycl.hpp)
set (BICYCL_MAIN_HEADER ${CMAKE_CURRENT_SOURCE_DIR}/interface.h)
set (BICYCL_INSTALLABLE_HEADERS_AND_INL
                      ${CMAKE_CURRENT_SOURCE_DIR}/bicycl/seclevel.hpp
                      ${CMAKE_CURRENT_SOURCE_DIR}/bicycl/seclevel.inl
                      ${CMAKE_CURRENT_SOURCE_DIR}/bicycl/gmp_extras.hpp
                      ${CMAKE_CURRENT_SOURCE_DIR}/bicycl/gmp_extras.inl
                      ${CMAKE_CURRENT_SOURCE_DIR}/bicycl/qfi.hpp
                      ${CMAKE_CURRENT_SOURCE_DIR}/bicycl/qfi.inl
                      ${CMAKE_CURRENT_SOURCE_DIR}/bicycl/CL_HSM_utils.hpp
                      ${CMAKE_CURRENT_SOURCE_DIR}/bicycl/CL_HSM_utils.inl
                      ${CMAKE_CURRENT_SOURCE_DIR}/bicycl/CL_HSMqk.hpp
                      ${CMAKE_CURRENT_SOURCE_DIR}/bicycl/CL_HSMqk.inl
                      ${CMAKE_CURRENT_SOURCE_DIR}/bicycl.hpp
                      )
add_library (bicycl ${CMAKE_CURRENT_SOURCE_DIR}/interface.cpp)
target_sources (bicycl PUBLIC ${BICYCL_MAIN_HEADER}
                                    ${BICYCL_INSTALLABLE_HEADERS_AND_INL})
target_include_directories (bicycl PUBLIC
                              $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
                              $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)
target_link_libraries (bicycl PUBLIC gmp::gmp)
if (THREADS_FOUND AND CMAKE_USE_PTHREADS_INIT)
  target_link_libraries (bicycl PUBLIC Threads::Threads)
  target_compile_definitions (bicycl PUBLIC BICYCL_WITH_PTHREADS)
endif ()

install (TARGETS bicycl EXPORT bicyclExport
                          RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
                          LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
                          ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR})
install (FILES ${BICYCL_MAIN_HEADER}
                          DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
install (FILES ${BICYCL_INSTALLABLE_HEADERS_AND_INL}
                          DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/bicycl)
