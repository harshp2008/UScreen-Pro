# Third-party components
- libevdi (host/evdi/evdi_lib.h, and libevdi.so.1 shipped next to the helper
  in release packages) is copyright DisplayLink (UK) Ltd. and licensed under
  the GNU Lesser General Public License v2.1 or later. It is used unmodified as
  a separate shared library. Source: https://github.com/DisplayLink/evdi
- The evdi kernel module is not part of this project; it is installed from
  your distribution or built from the same upstream repository.
- S Pen Remote Open SDK (android/spen-remote/, vendored unmodified apart from
  its build file) is copyright David Allison and licensed under the Apache
  License, Version 2.0 (android/spen-remote/LICENSE.txt). It is a clean-room
  reimplementation of the API of Samsung's proprietary S Pen Remote SDK, so no
  Samsung JARs are distributed. Source:
  https://github.com/david-allison/S-Pen-Remote-Open-SDK
