# riode-glass plugin

CXX ?= g++
CXXFLAGS = -fPIC -g -O2 -std=c++23
# Never ask for an executable stack: glibc 2.41+ refuses to load such a library.
LDFLAGS = -shared -Wl,-z,noexecstack
INCLUDES = $(shell pkg-config --cflags hyprland pixman-1 libdrm)
LIBS = $(shell pkg-config --libs hyprland) -ldl

# Shown by `hyprctl plugin list`.
ifndef RIODE_GLASS_VERSION
RIODE_GLASS_VERSION := 0.1.0
endif
CXXFLAGS += -DRIODE_GLASS_VERSION='"$(RIODE_GLASS_VERSION)"'

# Keeps two loaded copies from sharing their globals (GCC only).
ifneq ($(shell $(CXX) --version 2>/dev/null | grep -c 'Free Software Foundation'),0)
	CXXFLAGS += --no-gnu-unique
endif

TARGET = riode-glass.so
SOURCES = src/main.cpp src/GlassBlur.cpp src/ItemHints.cpp src/GlassOptics.cpp
OBJ = $(SOURCES:.cpp=.o)
HEADERS = $(wildcard src/*.hpp)

# ── riode_item_v1 protocol helper ────────────────────────────────────────────
# Built as its own shared library and embedded (via ItemHelperBlob.S) inside
# riode-glass.so, so it can be dlopen'd with RTLD_NODELETE and outlive a plugin
# reload — clients' item objects become inert instead of dangling. Depends only
# on libwayland-server, never on Hyprland headers. The server header is
# hand-written (no wayland-scanner in the closure); verify it against
# protocols/riode-item-v1.xml if either changes.

PROTOCOL_HELPER_DIR = src/item-helper
HELPER_SO           = $(PROTOCOL_HELPER_DIR)/riode-item-helper.so
HELPER_SOURCES      = $(PROTOCOL_HELPER_DIR)/helper.c
HELPER_OBJ          = $(HELPER_SOURCES:.c=.o)

WAYLAND_SERVER_CFLAGS = $(shell pkg-config --cflags wayland-server)
WAYLAND_SERVER_LIBS   = $(shell pkg-config --libs wayland-server)

ITEM_HELPER_BLOB_OBJ = src/ItemHelperBlob.o

all: $(TARGET)

%.o : %.cpp $(HEADERS)
	@echo "[$(CXX)] $<"
	@$(CXX) -c $(CXXFLAGS) $(INCLUDES) $< -o $@

$(PROTOCOL_HELPER_DIR)/%.o: $(PROTOCOL_HELPER_DIR)/%.c src/item-helper/riode-item-v1-server-protocol.h src/item-helper/riode_item_helper_api.h
	@echo "[$(CC)] $<"
	@$(CC) -c -fPIC -O2 -std=c11 $(WAYLAND_SERVER_CFLAGS) -I$(PROTOCOL_HELPER_DIR) $< -o $@

$(HELPER_SO): $(HELPER_OBJ)
	@echo "Linking $(HELPER_SO)..."
	@$(CC) -shared -fPIC -Wl,-z,noexecstack -o $@ $(HELPER_OBJ) $(WAYLAND_SERVER_LIBS)

$(ITEM_HELPER_BLOB_OBJ): src/ItemHelperBlob.S $(HELPER_SO)
	@echo "[as] $<"
	@$(CC) -c $< -o $@

$(TARGET): $(OBJ) $(ITEM_HELPER_BLOB_OBJ)
	@echo "Linking $(TARGET)..."
	@$(CXX) $(LDFLAGS) $(OBJ) $(ITEM_HELPER_BLOB_OBJ) -o $@ $(LIBS)
	@echo "Done!"

clean:
	rm -f $(OBJ) $(TARGET) $(ITEM_HELPER_BLOB_OBJ) $(HELPER_OBJ) $(HELPER_SO)

.PHONY: all clean
