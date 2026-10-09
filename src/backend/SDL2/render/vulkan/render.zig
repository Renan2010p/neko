// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `sdl2-vulkan` backend — an SDL2 window presenting through **Vulkan**.
//!
//! Like the OpenGL backend this targets the pygame translation layer: the game
//! renders into a CPU `Surface` and `display.flip` uploads it as one streaming
//! texture drawn as a full-screen quad. It implements window/events/timing, a
//! streaming texture and `present`; the engine's own primitives/`text`/`sound`
//! are not provided (the pygame path does not use them). Select with
//! `-Dbackend=sdl2-vulkan`.
//!
//! The SPIR-V shaders live in `shaders/` and are regenerated with
//! `glslangValidator -V shaders/quad.vert -o shaders/quad_vert.spv` (same for
//! `quad.frag`).

const std: type = @import("std");
const c: type = @import("c");
const engine: type = @import("neko");
const platform: type = @import("neko_sdl2_platform");
const render_common: type = @import("neko_sdl2_render");

const Allocator: type = std.mem.Allocator;

const Vertex: type = extern struct {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
};

const QUAD: [4]Vertex = .{
    .{ .x = 0, .y = 0, .u = 0, .v = 0 },
    .{ .x = 1, .y = 0, .u = 1, .v = 0 },
    .{ .x = 0, .y = 1, .u = 0, .v = 1 },
    .{ .x = 1, .y = 1, .u = 1, .v = 1 },
};

const PushConsts: type = extern struct {
    rect: [4]f32, // x0, y_top, x1, y_bottom (NDC)
    alpha: f32,
};

const vert_spv: [*]const u8 = @embedFile("shaders/quad_vert.spv");
const vert_len: usize = @embedFile("shaders/quad_vert.spv").len;
const frag_spv: [*]const u8 = @embedFile("shaders/quad_frag.spv");
const frag_len: usize = @embedFile("shaders/quad_frag.spv").len;

const VulkanEngine: type = struct {
    allocator: Allocator = undefined,
    /// Shared SDL window / events / timing / files layer.
    sdl: platform.Sdl = .{},
    /// Vulkan present mode follows this flag.
    vsync: bool = true,

    instance: c.VkInstance = null,
    surface: c.VkSurfaceKHR = null,
    physical: c.VkPhysicalDevice = null,
    device: c.VkDevice = null,
    queue: c.VkQueue = null,
    queue_family: u32 = 0,
    swapchain: c.VkSwapchainKHR = null,
    swap_format: c.VkFormat = c.VK_FORMAT_B8G8R8A8_UNORM,
    swap_extent: c.VkExtent2D = .{ .width = 0, .height = 0 },
    swap_views: []c.VkImageView = &.{},
    render_pass: c.VkRenderPass = null,
    pipeline_layout: c.VkPipelineLayout = null,
    pipeline: c.VkPipeline = null,
    framebuffers: []c.VkFramebuffer = &.{},
    desc_layout: c.VkDescriptorSetLayout = null,
    desc_pool: c.VkDescriptorPool = null,
    desc_set: c.VkDescriptorSet = null,
    cmd_pool: c.VkCommandPool = null,
    cmd: c.VkCommandBuffer = null,
    sem_available: c.VkSemaphore = null,
    sem_finished: c.VkSemaphore = null,
    fence: c.VkFence = null,
    vbuf: c.VkBuffer = null,
    vmem: c.VkDeviceMemory = null,

    tex_image: c.VkImage = null,
    tex_mem: c.VkDeviceMemory = null,
    tex_view: c.VkImageView = null,
    tex_sampler: c.VkSampler = null,
    staging: c.VkBuffer = null,
    staging_mem: c.VkDeviceMemory = null,
    staging_map: [*]u8 = undefined,
    staging_size: usize = 0,
    tex_w: u32 = 0,
    tex_h: u32 = 0,
    tex_dirty: bool = false,

    draw_rect: engine.Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },
    draw_alpha: f32 = 1.0,
    has_draw: bool = false,

    pub fn backend(self: *VulkanEngine) engine.Backend {
        return engine.Backend{
            .ptr = @ptrCast(self),
            .vtable = &vtable,
            .caps = caps_decl,
        };
    }
};

pub const Engine: type = VulkanEngine;
var instance: VulkanEngine = .{};

/// Which backend this module implements.
pub const kind: engine.BackendKind = .{ .name = "sdl2-vulkan" };

/// The capabilities this backend declares.
const caps_decl: engine.Capabilities = blk: {
    var set: engine.Capabilities = engine.Capabilities.initEmpty();
    set.insert(.graphics2d);
    set.insert(.input);
    set.insert(.files);
    break :blk set;
};

/// Shared entries from `platform.zig`, reading `VulkanEngine.sdl`.
const P: type = platform.adapter(VulkanEngine, "sdl");

/// Returns the backend as an abstract handle. Called by `src/core/platform.zig`.
pub fn create() engine.Backend {
    return instance.backend();
}

fn as_self(ptr: *anyopaque) *VulkanEngine {
    return @ptrCast(@alignCast(ptr));
}

fn ok(rc: c.VkResult) bool {
    return rc == c.VK_SUCCESS;
}

fn cstr(ptr: [*c]const u8) []const u8 {
    if (ptr == null) return "";
    return std.mem.span(@as([*:0]const u8, @ptrCast(ptr)));
}

// ── Vulkan helpers ───────────────────────────────────────────────────────────

fn findMemoryType(self: *VulkanEngine, type_bits: u32, props: c.VkMemoryPropertyFlags) ?u32 {
    var mp: c.VkPhysicalDeviceMemoryProperties = undefined;
    c.vkGetPhysicalDeviceMemoryProperties(self.physical, &mp);
    var i: u32 = 0;
    while (i < mp.memoryTypeCount) : (i += 1) {
        const bit: u32 = @as(u32, 1) << @intCast(i);
        if ((type_bits & bit) != 0 and (mp.memoryTypes[i].propertyFlags & props) == props) return i;
    }
    return null;
}

fn createBuffer(
    self: *VulkanEngine,
    size: u64,
    usage: c.VkBufferUsageFlags,
    props: c.VkMemoryPropertyFlags,
    out_buf: *c.VkBuffer,
    out_mem: *c.VkDeviceMemory,
    out_map: ?*?[*]u8,
) bool {
    var info: c.VkBufferCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        .size = size,
        .usage = usage,
        .sharingMode = c.VK_SHARING_MODE_EXCLUSIVE,
    };
    if (!ok(c.vkCreateBuffer(self.device, &info, null, out_buf))) return false;
    var req: c.VkMemoryRequirements = undefined;
    c.vkGetBufferMemoryRequirements(self.device, out_buf.*, &req);
    const mt = findMemoryType(self, req.memoryTypeBits, props) orelse return false;
    var alloc: c.VkMemoryAllocateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize = req.size,
        .memoryTypeIndex = mt,
    };
    if (!ok(c.vkAllocateMemory(self.device, &alloc, null, out_mem))) return false;
    if (!ok(c.vkBindBufferMemory(self.device, out_buf.*, out_mem.*, 0))) return false;
    if (out_map) |m| {
        var ptr: ?*anyopaque = null;
        if (!ok(c.vkMapMemory(self.device, out_mem.*, 0, c.VK_WHOLE_SIZE, 0, &ptr))) return false;
        m.* = @ptrCast(ptr);
    }
    return true;
}

fn createShaderModule(self: *VulkanEngine, data: [*]const u8, len: usize) ?c.VkShaderModule {
    const words: []u32 = self.allocator.alloc(u32, (len + 3) / 4) catch return null;
    defer self.allocator.free(words);
    @memcpy(std.mem.sliceAsBytes(words)[0..len], data[0..len]);
    var info: c.VkShaderModuleCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        .codeSize = len,
        .pCode = words.ptr,
    };
    var module: c.VkShaderModule = null;
    if (!ok(c.vkCreateShaderModule(self.device, &info, null, &module))) return null;
    return module;
}

/// (Re)creates the streaming canvas texture and its staging buffer. When
/// `out_map` is null the staging buffer is not mapped (used for the dummy).
fn createTexture(self: *VulkanEngine, width: u32, height: u32) bool {
    if (width == 0 or height == 0) return false;
    destroyTexture(self);

    var img_info: c.VkImageCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
        .imageType = c.VK_IMAGE_TYPE_2D,
        .format = c.VK_FORMAT_B8G8R8A8_UNORM,
        .extent = .{ .width = width, .height = height, .depth = 1 },
        .mipLevels = 1,
        .arrayLayers = 1,
        .samples = c.VK_SAMPLE_COUNT_1_BIT,
        .tiling = c.VK_IMAGE_TILING_OPTIMAL,
        .usage = c.VK_IMAGE_USAGE_TRANSFER_DST_BIT | c.VK_IMAGE_USAGE_SAMPLED_BIT,
        .sharingMode = c.VK_SHARING_MODE_EXCLUSIVE,
        .initialLayout = c.VK_IMAGE_LAYOUT_UNDEFINED,
    };
    if (!ok(c.vkCreateImage(self.device, &img_info, null, &self.tex_image))) return false;

    var req: c.VkMemoryRequirements = undefined;
    c.vkGetImageMemoryRequirements(self.device, self.tex_image, &req);
    const mt = findMemoryType(self, req.memoryTypeBits, c.VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT) orelse return false;
    var alloc: c.VkMemoryAllocateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize = req.size,
        .memoryTypeIndex = mt,
    };
    if (!ok(c.vkAllocateMemory(self.device, &alloc, null, &self.tex_mem))) return false;
    if (!ok(c.vkBindImageMemory(self.device, self.tex_image, self.tex_mem, 0))) return false;

    var view_info: c.VkImageViewCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
        .image = self.tex_image,
        .viewType = c.VK_IMAGE_VIEW_TYPE_2D,
        .format = c.VK_FORMAT_B8G8R8A8_UNORM,
        .subresourceRange = .{
            .aspectMask = c.VK_IMAGE_ASPECT_COLOR_BIT,
            .baseMipLevel = 0,
            .levelCount = 1,
            .baseArrayLayer = 0,
            .layerCount = 1,
        },
    };
    if (!ok(c.vkCreateImageView(self.device, &view_info, null, &self.tex_view))) return false;

    const size: u64 = @as(u64, width) * height * 4;
    var map_ptr: ?[*]u8 = null;
    if (!createBuffer(self, size, c.VK_BUFFER_USAGE_TRANSFER_SRC_BIT, c.VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | c.VK_MEMORY_PROPERTY_HOST_COHERENT_BIT, &self.staging, &self.staging_mem, &map_ptr)) return false;
    self.staging_map = map_ptr.?;
    self.staging_size = @intCast(size);
    self.tex_w = width;
    self.tex_h = height;
    self.tex_dirty = false;
    updateDescriptor(self);
    return true;
}

fn destroyTexture(self: *VulkanEngine) void {
    if (self.device == null) return;
    if (self.staging != null) {
        c.vkDestroyBuffer(self.device, self.staging, null);
        self.staging = null;
    }
    if (self.staging_mem != null) {
        c.vkFreeMemory(self.device, self.staging_mem, null);
        self.staging_mem = null;
    }
    if (self.tex_view != null) {
        c.vkDestroyImageView(self.device, self.tex_view, null);
        self.tex_view = null;
    }
    if (self.tex_image != null) {
        c.vkDestroyImage(self.device, self.tex_image, null);
        self.tex_image = null;
    }
    if (self.tex_mem != null) {
        c.vkFreeMemory(self.device, self.tex_mem, null);
        self.tex_mem = null;
    }
}

fn updateDescriptor(self: *VulkanEngine) void {
    if (self.desc_set == null or self.tex_view == null) return;
    var img: c.VkDescriptorImageInfo = .{
        .sampler = self.tex_sampler,
        .imageView = self.tex_view,
        .imageLayout = c.VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
    };
    var write: c.VkWriteDescriptorSet = .{
        .sType = c.VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
        .dstSet = self.desc_set,
        .dstBinding = 0,
        .descriptorCount = 1,
        .descriptorType = c.VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
        .pImageInfo = &img,
    };
    c.vkUpdateDescriptorSets(self.device, 1, &write, 0, null);
}

fn createSwapchain(self: *VulkanEngine) bool {
    var caps: c.VkSurfaceCapabilitiesKHR = undefined;
    if (!ok(c.vkGetPhysicalDeviceSurfaceCapabilitiesKHR(self.physical, self.surface, &caps))) return false;

    // Format: prefer B8G8R8A8_UNORM (matches our CPU pixels).
    var format_count: u32 = 0;
    _ = c.vkGetPhysicalDeviceSurfaceFormatsKHR(self.physical, self.surface, &format_count, null);
    var formats: [64]c.VkSurfaceFormatKHR = undefined;
    if (format_count > formats.len) format_count = formats.len;
    _ = c.vkGetPhysicalDeviceSurfaceFormatsKHR(self.physical, self.surface, &format_count, &formats);
    self.swap_format = formats[0].format;
    for (formats[0..format_count]) |f| {
        if (f.format == c.VK_FORMAT_B8G8R8A8_UNORM) {
            self.swap_format = f.format;
            break;
        }
    }

    // Present mode.
    var mode_count: u32 = 0;
    _ = c.vkGetPhysicalDeviceSurfacePresentModesKHR(self.physical, self.surface, &mode_count, null);
    var modes: [32]c.VkPresentModeKHR = undefined;
    if (mode_count > modes.len) mode_count = modes.len;
    _ = c.vkGetPhysicalDeviceSurfacePresentModesKHR(self.physical, self.surface, &mode_count, &modes);
    var present_mode: c.VkPresentModeKHR = c.VK_PRESENT_MODE_FIFO_KHR;
    if (!self.vsync) {
        for (modes[0..mode_count]) |m| {
            if (m == c.VK_PRESENT_MODE_MAILBOX_KHR) {
                present_mode = m;
                break;
            }
            if (m == c.VK_PRESENT_MODE_IMMEDIATE_KHR) present_mode = m;
        }
    }

    // Extent.
    if (caps.currentExtent.width != 0xFFFFFFFF and caps.currentExtent.height != 0xFFFFFFFF) {
        self.swap_extent = caps.currentExtent;
    } else {
        var w: c_int = 0;
        var h: c_int = 0;
        c.SDL_Vulkan_GetDrawableSize(self.sdl.window, &w, &h);
        self.swap_extent = .{ .width = @intCast(w), .height = @intCast(h) };
    }
    if (self.swap_extent.width == 0 or self.swap_extent.height == 0) return false;

    var image_count: u32 = caps.minImageCount + 1;
    if (caps.maxImageCount > 0 and image_count > caps.maxImageCount) image_count = caps.maxImageCount;

    var info: c.VkSwapchainCreateInfoKHR = .{
        .sType = c.VK_STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO_KHR,
        .surface = self.surface,
        .minImageCount = image_count,
        .imageFormat = self.swap_format,
        .imageColorSpace = formats[0].colorSpace,
        .imageExtent = self.swap_extent,
        .imageArrayLayers = 1,
        .imageUsage = c.VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT,
        .imageSharingMode = c.VK_SHARING_MODE_EXCLUSIVE,
        .preTransform = caps.currentTransform,
        .compositeAlpha = c.VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR,
        .presentMode = present_mode,
        .clipped = c.VK_TRUE,
        .oldSwapchain = null,
    };
    if (!ok(c.vkCreateSwapchainKHR(self.device, &info, null, &self.swapchain))) return false;

    var count: u32 = 0;
    _ = c.vkGetSwapchainImagesKHR(self.device, self.swapchain, &count, null);
    const images = self.allocator.alloc(c.VkImage, count) catch return false;
    _ = c.vkGetSwapchainImagesKHR(self.device, self.swapchain, &count, images.ptr);

    self.swap_views = self.allocator.alloc(c.VkImageView, count) catch return false;
    for (images, 0..) |img, i| {
        var view_info: c.VkImageViewCreateInfo = .{
            .sType = c.VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
            .image = img,
            .viewType = c.VK_IMAGE_VIEW_TYPE_2D,
            .format = self.swap_format,
            .subresourceRange = .{
                .aspectMask = c.VK_IMAGE_ASPECT_COLOR_BIT,
                .baseMipLevel = 0,
                .levelCount = 1,
                .baseArrayLayer = 0,
                .layerCount = 1,
            },
        };
        if (!ok(c.vkCreateImageView(self.device, &view_info, null, &self.swap_views[i]))) return false;
    }
    self.allocator.free(images);

    self.framebuffers = self.allocator.alloc(c.VkFramebuffer, count) catch return false;
    for (self.swap_views, 0..) |view, i| {
        var fb_info: c.VkFramebufferCreateInfo = .{
            .sType = c.VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO,
            .renderPass = self.render_pass,
            .attachmentCount = 1,
            .pAttachments = &view,
            .width = self.swap_extent.width,
            .height = self.swap_extent.height,
            .layers = 1,
        };
        if (!ok(c.vkCreateFramebuffer(self.device, &fb_info, null, &self.framebuffers[i]))) return false;
    }
    return true;
}

fn destroySwapchain(self: *VulkanEngine) void {
    if (self.device == null) return;
    for (self.framebuffers) |fb| c.vkDestroyFramebuffer(self.device, fb, null);
    if (self.framebuffers.len > 0) self.allocator.free(self.framebuffers);
    self.framebuffers = &.{};
    for (self.swap_views) |v| c.vkDestroyImageView(self.device, v, null);
    if (self.swap_views.len > 0) self.allocator.free(self.swap_views);
    self.swap_views = &.{};
    if (self.swapchain != null) {
        c.vkDestroySwapchainKHR(self.device, self.swapchain, null);
        self.swapchain = null;
    }
}

fn recreateSwapchain(self: *VulkanEngine) void {
    _ = c.vkDeviceWaitIdle(self.device);
    destroySwapchain(self);
    _ = createSwapchain(self);
    self.has_draw = false;
}

fn createRenderPass(self: *VulkanEngine) bool {
    var color: c.VkAttachmentDescription = .{
        .format = self.swap_format,
        .samples = c.VK_SAMPLE_COUNT_1_BIT,
        .loadOp = c.VK_ATTACHMENT_LOAD_OP_CLEAR,
        .storeOp = c.VK_ATTACHMENT_STORE_OP_STORE,
        .stencilLoadOp = c.VK_ATTACHMENT_LOAD_OP_DONT_CARE,
        .stencilStoreOp = c.VK_ATTACHMENT_STORE_OP_DONT_CARE,
        .initialLayout = c.VK_IMAGE_LAYOUT_UNDEFINED,
        .finalLayout = c.VK_IMAGE_LAYOUT_PRESENT_SRC_KHR,
    };
    var color_ref: c.VkAttachmentReference = .{
        .attachment = 0,
        .layout = c.VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
    };
    var subpass: c.VkSubpassDescription = .{
        .pipelineBindPoint = c.VK_PIPELINE_BIND_POINT_GRAPHICS,
        .colorAttachmentCount = 1,
        .pColorAttachments = &color_ref,
    };
    var dep: c.VkSubpassDependency = .{
        .srcSubpass = c.VK_SUBPASS_EXTERNAL,
        .dstSubpass = 0,
        .srcStageMask = c.VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
        .dstStageMask = c.VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
        .dstAccessMask = c.VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT,
    };
    var info: c.VkRenderPassCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO,
        .attachmentCount = 1,
        .pAttachments = &color,
        .subpassCount = 1,
        .pSubpasses = &subpass,
        .dependencyCount = 1,
        .pDependencies = &dep,
    };
    return ok(c.vkCreateRenderPass(self.device, &info, null, &self.render_pass));
}

fn createGraphicsPipeline(self: *VulkanEngine) bool {
    const vert = createShaderModule(self, vert_spv, vert_len) orelse return false;
    defer c.vkDestroyShaderModule(self.device, vert, null);
    const frag = createShaderModule(self, frag_spv, frag_len) orelse return false;
    defer c.vkDestroyShaderModule(self.device, frag, null);

    var stages = [_]c.VkPipelineShaderStageCreateInfo{
        .{
            .sType = c.VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
            .stage = c.VK_SHADER_STAGE_VERTEX_BIT,
            .module = vert,
            .pName = "main",
        },
        .{
            .sType = c.VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
            .stage = c.VK_SHADER_STAGE_FRAGMENT_BIT,
            .module = frag,
            .pName = "main",
        },
    };

    var binding: c.VkVertexInputBindingDescription = .{
        .binding = 0,
        .stride = @sizeOf(Vertex),
        .inputRate = c.VK_VERTEX_INPUT_RATE_VERTEX,
    };
    var attrs = [_]c.VkVertexInputAttributeDescription{
        .{ .location = 0, .binding = 0, .format = c.VK_FORMAT_R32G32_SFLOAT, .offset = 0 },
        .{ .location = 1, .binding = 0, .format = c.VK_FORMAT_R32G32_SFLOAT, .offset = @sizeOf([2]f32) },
    };
    var vertex_input: c.VkPipelineVertexInputStateCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
        .vertexBindingDescriptionCount = 1,
        .pVertexBindingDescriptions = &binding,
        .vertexAttributeDescriptionCount = 2,
        .pVertexAttributeDescriptions = &attrs,
    };
    var input_assembly: c.VkPipelineInputAssemblyStateCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
        .topology = c.VK_PRIMITIVE_TOPOLOGY_TRIANGLE_STRIP,
    };
    var viewport_state: c.VkPipelineViewportStateCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO,
        .viewportCount = 1,
        .scissorCount = 1,
    };
    var raster: c.VkPipelineRasterizationStateCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
        .polygonMode = c.VK_POLYGON_MODE_FILL,
        .cullMode = c.VK_CULL_MODE_NONE,
        .frontFace = c.VK_FRONT_FACE_COUNTER_CLOCKWISE,
        .lineWidth = 1.0,
    };
    var multisample: c.VkPipelineMultisampleStateCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
        .rasterizationSamples = c.VK_SAMPLE_COUNT_1_BIT,
    };
    var blend_attach: c.VkPipelineColorBlendAttachmentState = .{
        .blendEnable = c.VK_TRUE,
        .srcColorBlendFactor = c.VK_BLEND_FACTOR_SRC_ALPHA,
        .dstColorBlendFactor = c.VK_BLEND_FACTOR_ONE_MINUS_SRC_ALPHA,
        .colorBlendOp = c.VK_BLEND_OP_ADD,
        .srcAlphaBlendFactor = c.VK_BLEND_FACTOR_ONE,
        .dstAlphaBlendFactor = c.VK_BLEND_FACTOR_ZERO,
        .alphaBlendOp = c.VK_BLEND_OP_ADD,
        .colorWriteMask = c.VK_COLOR_COMPONENT_R_BIT | c.VK_COLOR_COMPONENT_G_BIT | c.VK_COLOR_COMPONENT_B_BIT | c.VK_COLOR_COMPONENT_A_BIT,
    };
    var blend: c.VkPipelineColorBlendStateCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
        .attachmentCount = 1,
        .pAttachments = &blend_attach,
    };
    var dynamic_states = [_]c.VkDynamicState{ c.VK_DYNAMIC_STATE_VIEWPORT, c.VK_DYNAMIC_STATE_SCISSOR };
    var dynamic: c.VkPipelineDynamicStateCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO,
        .dynamicStateCount = dynamic_states.len,
        .pDynamicStates = &dynamic_states,
    };
    var push: c.VkPushConstantRange = .{
        .stageFlags = c.VK_SHADER_STAGE_VERTEX_BIT | c.VK_SHADER_STAGE_FRAGMENT_BIT,
        .offset = 0,
        .size = @sizeOf(PushConsts),
    };
    var layout_info: c.VkPipelineLayoutCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
        .setLayoutCount = 1,
        .pSetLayouts = &self.desc_layout,
        .pushConstantRangeCount = 1,
        .pPushConstantRanges = &push,
    };
    if (!ok(c.vkCreatePipelineLayout(self.device, &layout_info, null, &self.pipeline_layout))) return false;

    var pipeline_info: c.VkGraphicsPipelineCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
        .stageCount = stages.len,
        .pStages = &stages,
        .pVertexInputState = &vertex_input,
        .pInputAssemblyState = &input_assembly,
        .pViewportState = &viewport_state,
        .pRasterizationState = &raster,
        .pMultisampleState = &multisample,
        .pColorBlendState = &blend,
        .pDynamicState = &dynamic,
        .layout = self.pipeline_layout,
        .renderPass = self.render_pass,
        .subpass = 0,
    };
    return ok(c.vkCreateGraphicsPipelines(self.device, null, 1, &pipeline_info, null, &self.pipeline));
}

// ── Vtable ───────────────────────────────────────────────────────────────────

const vtable: engine.Backend.VTable = .{
    .init = vt_init,
    .shutdown = vt_shutdown,
    .keeps_running = P.keeps_running,
    .request_stop = P.request_stop,
    .present = vt_present,
    .poll_event = P.poll_event,
    .ticks_ms = P.ticks_ms,
    .set_logical_size = P.set_logical_size,
    .set_title = P.set_title,
    .set_fullscreen = vt_set_fullscreen,
    .set_vsync = vt_set_vsync,
    .set_resolution = P.set_resolution,
    .logical_size = P.logical_size,
    .display_modes = P.display_modes,
    .supports_curved_panorama = vt_supports_curved_panorama,
    .supports_offscreen_targets = vt_supports_offscreen_targets,
    .set_draw_offset = P.set_draw_offset,
    .create_texture = vt_create_texture,
    .update_texture = vt_update_texture,
    .draw_texture = vt_draw_texture,
    .draw_texture_rotated = vt_draw_texture_rotated,
    .texture_size = vt_texture_size,
    .mouse_pos = P.mouse_pos,
    .read_file = P.read_file,
    .write_file = P.write_file,
    .delete_file = P.delete_file,
    .file_exists = P.file_exists,
};

fn vt_init(ptr: *anyopaque, config: engine.Config) bool {
    const self: *VulkanEngine = as_self(ptr);
    self.allocator = config.allocator;
    self.vsync = config.vsync;

    if (!self.sdl.begin(config, c.SDL_INIT_VIDEO)) return false;
    if (!self.sdl.openWindow(config, c.SDL_WINDOW_VULKAN | c.SDL_WINDOW_SHOWN)) return false;

    // Instance extensions requested by SDL.
    var ext_count: c_uint = 0;
    if (c.SDL_Vulkan_GetInstanceExtensions(self.sdl.window, &ext_count, null) == 0) return false;
    const exts = self.allocator.alloc([*c]const u8, ext_count) catch return false;
    defer self.allocator.free(exts);
    if (c.SDL_Vulkan_GetInstanceExtensions(self.sdl.window, &ext_count, exts.ptr) == 0) return false;

    var app_info: c.VkApplicationInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_APPLICATION_INFO,
        .pApplicationName = "neko",
        .pEngineName = "neko",
        .apiVersion = c.VK_API_VERSION_1_0,
    };
    var inst_info: c.VkInstanceCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
        .pApplicationInfo = &app_info,
        .enabledExtensionCount = ext_count,
        .ppEnabledExtensionNames = exts.ptr,
    };
    if (!ok(c.vkCreateInstance(&inst_info, null, &self.instance))) return false;
    if (c.SDL_Vulkan_CreateSurface(self.sdl.window, self.instance, &self.surface) == 0) return false;

    // Physical device + queue family.
    var dev_count: u32 = 0;
    _ = c.vkEnumeratePhysicalDevices(self.instance, &dev_count, null);
    if (dev_count == 0) return false;
    const devices = self.allocator.alloc(c.VkPhysicalDevice, dev_count) catch return false;
    defer self.allocator.free(devices);
    _ = c.vkEnumeratePhysicalDevices(self.instance, &dev_count, devices.ptr);
    self.physical = devices[0];
    for (devices) |dev| {
        var qcount: u32 = 0;
        c.vkGetPhysicalDeviceQueueFamilyProperties(dev, &qcount, null);
        const qprops = self.allocator.alloc(c.VkQueueFamilyProperties, qcount) catch continue;
        defer self.allocator.free(qprops);
        c.vkGetPhysicalDeviceQueueFamilyProperties(dev, &qcount, qprops.ptr);
        var found = false;
        for (qprops, 0..) |qp, i| {
            if ((qp.queueFlags & c.VK_QUEUE_GRAPHICS_BIT) == 0) continue;
            var present: c.VkBool32 = 0;
            _ = c.vkGetPhysicalDeviceSurfaceSupportKHR(dev, @intCast(i), self.surface, &present);
            if (present != 0) {
                self.physical = dev;
                self.queue_family = @intCast(i);
                found = true;
                break;
            }
        }
        if (found) break;
    }

    const prio: f32 = 1.0;
    var qinfo: c.VkDeviceQueueCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
        .queueFamilyIndex = self.queue_family,
        .queueCount = 1,
        .pQueuePriorities = &prio,
    };
    const dev_exts = [_][*c]const u8{c.VK_KHR_SWAPCHAIN_EXTENSION_NAME};
    var dinfo: c.VkDeviceCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
        .queueCreateInfoCount = 1,
        .pQueueCreateInfos = &qinfo,
        .enabledExtensionCount = dev_exts.len,
        .ppEnabledExtensionNames = &dev_exts,
    };
    if (!ok(c.vkCreateDevice(self.physical, &dinfo, null, &self.device))) return false;
    c.vkGetDeviceQueue(self.device, self.queue_family, 0, &self.queue);

    // Sampler + descriptor set layout.
    var sampler_info: c.VkSamplerCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_SAMPLER_CREATE_INFO,
        .magFilter = c.VK_FILTER_NEAREST,
        .minFilter = c.VK_FILTER_NEAREST,
        .mipmapMode = c.VK_SAMPLER_MIPMAP_MODE_NEAREST,
        .addressModeU = c.VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        .addressModeV = c.VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        .addressModeW = c.VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
    };
    if (!ok(c.vkCreateSampler(self.device, &sampler_info, null, &self.tex_sampler))) return false;

    var bind: c.VkDescriptorSetLayoutBinding = .{
        .binding = 0,
        .descriptorType = c.VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
        .descriptorCount = 1,
        .stageFlags = c.VK_SHADER_STAGE_FRAGMENT_BIT,
    };
    var dsl: c.VkDescriptorSetLayoutCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
        .bindingCount = 1,
        .pBindings = &bind,
    };
    if (!ok(c.vkCreateDescriptorSetLayout(self.device, &dsl, null, &self.desc_layout))) return false;

    if (!createRenderPass(self)) return false;
    if (!createGraphicsPipeline(self)) return false;
    if (!createSwapchain(self)) return false;

    // Descriptor pool + set.
    var pool_size: c.VkDescriptorPoolSize = .{
        .type = c.VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
        .descriptorCount = 1,
    };
    var pool_info: c.VkDescriptorPoolCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
        .maxSets = 1,
        .poolSizeCount = 1,
        .pPoolSizes = &pool_size,
    };
    if (!ok(c.vkCreateDescriptorPool(self.device, &pool_info, null, &self.desc_pool))) return false;
    var set_info: c.VkDescriptorSetAllocateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
        .descriptorPool = self.desc_pool,
        .descriptorSetCount = 1,
        .pSetLayouts = &self.desc_layout,
    };
    if (!ok(c.vkAllocateDescriptorSets(self.device, &set_info, &self.desc_set))) return false;

    // Command pool + buffer.
    var cmd_pool_info: c.VkCommandPoolCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
        .flags = c.VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT,
        .queueFamilyIndex = self.queue_family,
    };
    if (!ok(c.vkCreateCommandPool(self.device, &cmd_pool_info, null, &self.cmd_pool))) return false;
    var cmd_alloc: c.VkCommandBufferAllocateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
        .commandPool = self.cmd_pool,
        .level = c.VK_COMMAND_BUFFER_LEVEL_PRIMARY,
        .commandBufferCount = 1,
    };
    if (!ok(c.vkAllocateCommandBuffers(self.device, &cmd_alloc, &self.cmd))) return false;

    // Vertex buffer (static unit quad).
    if (!createBuffer(self, @sizeOf([4]Vertex), c.VK_BUFFER_USAGE_VERTEX_BUFFER_BIT, c.VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | c.VK_MEMORY_PROPERTY_HOST_COHERENT_BIT, &self.vbuf, &self.vmem, null)) return false;
    var vmap: ?*anyopaque = null;
    if (!ok(c.vkMapMemory(self.device, self.vmem, 0, c.VK_WHOLE_SIZE, 0, &vmap))) return false;
    @memcpy(std.mem.sliceAsBytes(@as([*]Vertex, @ptrCast(@alignCast(vmap.?)))[0..4]), std.mem.sliceAsBytes(&QUAD));

    // Sync.
    var sem_info: c.VkSemaphoreCreateInfo = .{ .sType = c.VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO };
    if (!ok(c.vkCreateSemaphore(self.device, &sem_info, null, &self.sem_available))) return false;
    if (!ok(c.vkCreateSemaphore(self.device, &sem_info, null, &self.sem_finished))) return false;
    var fence_info: c.VkFenceCreateInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_FENCE_CREATE_INFO,
        .flags = c.VK_FENCE_CREATE_SIGNALED_BIT,
    };
    if (!ok(c.vkCreateFence(self.device, &fence_info, null, &self.fence))) return false;

    // Initial 1x1 texture so the descriptor set is valid.
    if (!createTexture(self, 1, 1)) return false;

    engine.log.info("vulkan: window {d}x{d} ready (swapchain {}x{})", .{ config.width, config.height, self.swap_extent.width, self.swap_extent.height });
    engine.attach(self.backend());
    return true;
}

fn vt_shutdown(ptr: *anyopaque) void {
    const self: *VulkanEngine = as_self(ptr);
    if (self.device != null) _ = c.vkDeviceWaitIdle(self.device);
    destroyTexture(self);
    destroySwapchain(self);
    if (self.device != null) {
        if (self.fence != null) c.vkDestroyFence(self.device, self.fence, null);
        if (self.sem_available != null) c.vkDestroySemaphore(self.device, self.sem_available, null);
        if (self.sem_finished != null) c.vkDestroySemaphore(self.device, self.sem_finished, null);
        if (self.vbuf != null) c.vkDestroyBuffer(self.device, self.vbuf, null);
        if (self.vmem != null) c.vkFreeMemory(self.device, self.vmem, null);
        if (self.cmd_pool != null) c.vkDestroyCommandPool(self.device, self.cmd_pool, null);
        if (self.desc_pool != null) c.vkDestroyDescriptorPool(self.device, self.desc_pool, null);
        if (self.desc_layout != null) c.vkDestroyDescriptorSetLayout(self.device, self.desc_layout, null);
        if (self.pipeline != null) c.vkDestroyPipeline(self.device, self.pipeline, null);
        if (self.pipeline_layout != null) c.vkDestroyPipelineLayout(self.device, self.pipeline_layout, null);
        if (self.render_pass != null) c.vkDestroyRenderPass(self.device, self.render_pass, null);
        if (self.tex_sampler != null) c.vkDestroySampler(self.device, self.tex_sampler, null);
        c.vkDestroyDevice(self.device, null);
        self.device = null;
    }
    if (self.surface != null) c.vkDestroySurfaceKHR(self.instance, self.surface, null);
    if (self.instance != null) c.vkDestroyInstance(self.instance, null);
    self.sdl.closeWindow();
    c.SDL_Quit();
    engine.detach();
}

fn vt_present(ptr: *anyopaque) void {
    const self: *VulkanEngine = as_self(ptr);
    if (self.device == null or self.swapchain == null) return;

    _ = c.vkWaitForFences(self.device, 1, &self.fence, c.VK_TRUE, std.math.maxInt(u64));

    var image_index: u32 = 0;
    const acq = c.vkAcquireNextImageKHR(self.device, self.swapchain, std.math.maxInt(u64), self.sem_available, null, &image_index);
    if (acq == c.VK_ERROR_OUT_OF_DATE_KHR) {
        recreateSwapchain(self);
        return;
    }
    if (acq != c.VK_SUCCESS and acq != c.VK_SUBOPTIMAL_KHR) return;

    _ = c.vkResetFences(self.device, 1, &self.fence);
    _ = c.vkResetCommandBuffer(self.cmd, 0);

    var begin: c.VkCommandBufferBeginInfo = .{ .sType = c.VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO, .flags = c.VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT };
    _ = c.vkBeginCommandBuffer(self.cmd, &begin);

    // Upload the streaming texture (if dirty).
    if (self.tex_dirty and self.tex_w > 0 and self.tex_h > 0) {
        var to_dst: c.VkImageMemoryBarrier = .{
            .sType = c.VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
            .oldLayout = c.VK_IMAGE_LAYOUT_UNDEFINED,
            .newLayout = c.VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
            .srcQueueFamilyIndex = c.VK_QUEUE_FAMILY_IGNORED,
            .dstQueueFamilyIndex = c.VK_QUEUE_FAMILY_IGNORED,
            .image = self.tex_image,
            .subresourceRange = .{ .aspectMask = c.VK_IMAGE_ASPECT_COLOR_BIT, .baseMipLevel = 0, .levelCount = 1, .baseArrayLayer = 0, .layerCount = 1 },
            .dstAccessMask = c.VK_ACCESS_TRANSFER_WRITE_BIT,
        };
        c.vkCmdPipelineBarrier(self.cmd, c.VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, c.VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, null, 0, null, 1, &to_dst);
        var region: c.VkBufferImageCopy = .{
            .bufferOffset = 0,
            .bufferRowLength = 0,
            .bufferImageHeight = 0,
            .imageSubresource = .{ .aspectMask = c.VK_IMAGE_ASPECT_COLOR_BIT, .mipLevel = 0, .baseArrayLayer = 0, .layerCount = 1 },
            .imageOffset = .{ .x = 0, .y = 0, .z = 0 },
            .imageExtent = .{ .width = self.tex_w, .height = self.tex_h, .depth = 1 },
        };
        c.vkCmdCopyBufferToImage(self.cmd, self.staging, self.tex_image, c.VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, 1, &region);
        var to_read: c.VkImageMemoryBarrier = to_dst;
        to_read.oldLayout = c.VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL;
        to_read.newLayout = c.VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL;
        to_read.srcAccessMask = c.VK_ACCESS_TRANSFER_WRITE_BIT;
        to_read.dstAccessMask = c.VK_ACCESS_SHADER_READ_BIT;
        c.vkCmdPipelineBarrier(self.cmd, c.VK_PIPELINE_STAGE_TRANSFER_BIT, c.VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT, 0, 0, null, 0, null, 1, &to_read);
        self.tex_dirty = false;
    }

    var clear_value: c.VkClearValue = .{ .color = .{ .float32 = .{ 0.0, 0.0, 0.0, 1.0 } } };
    var rp_begin: c.VkRenderPassBeginInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO,
        .renderPass = self.render_pass,
        .framebuffer = self.framebuffers[image_index],
        .renderArea = .{ .offset = .{ .x = 0, .y = 0 }, .extent = self.swap_extent },
        .clearValueCount = 1,
        .pClearValues = &clear_value,
    };
    c.vkCmdBeginRenderPass(self.cmd, &rp_begin, c.VK_SUBPASS_CONTENTS_INLINE);

    if (self.has_draw and self.tex_view != null) {
        var viewport: c.VkViewport = .{
            .x = 0,
            .y = 0,
            .width = @floatFromInt(self.swap_extent.width),
            .height = @floatFromInt(self.swap_extent.height),
            .minDepth = 0,
            .maxDepth = 1,
        };
        c.vkCmdSetViewport(self.cmd, 0, 1, &viewport);
        var scissor: c.VkRect2D = .{ .offset = .{ .x = 0, .y = 0 }, .extent = self.swap_extent };
        c.vkCmdSetScissor(self.cmd, 0, 1, &scissor);
        c.vkCmdBindPipeline(self.cmd, c.VK_PIPELINE_BIND_POINT_GRAPHICS, self.pipeline);
        var offset: c.VkDeviceSize = 0;
        c.vkCmdBindVertexBuffers(self.cmd, 0, 1, &self.vbuf, &offset);
        c.vkCmdBindDescriptorSets(self.cmd, c.VK_PIPELINE_BIND_POINT_GRAPHICS, self.pipeline_layout, 0, 1, &self.desc_set, 0, null);
        var pc: PushConsts = .{ .rect = rectNdc(self), .alpha = self.draw_alpha };
        c.vkCmdPushConstants(self.cmd, self.pipeline_layout, c.VK_SHADER_STAGE_VERTEX_BIT | c.VK_SHADER_STAGE_FRAGMENT_BIT, 0, @sizeOf(PushConsts), &pc);
        c.vkCmdDraw(self.cmd, 4, 1, 0, 0);
    }
    c.vkCmdEndRenderPass(self.cmd);
    _ = c.vkEndCommandBuffer(self.cmd);

    const wait_stage = [_]c.VkPipelineStageFlags{c.VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT};
    var submit: c.VkSubmitInfo = .{
        .sType = c.VK_STRUCTURE_TYPE_SUBMIT_INFO,
        .waitSemaphoreCount = 1,
        .pWaitSemaphores = &self.sem_available,
        .pWaitDstStageMask = &wait_stage,
        .commandBufferCount = 1,
        .pCommandBuffers = &self.cmd,
        .signalSemaphoreCount = 1,
        .pSignalSemaphores = &self.sem_finished,
    };
    if (!ok(c.vkQueueSubmit(self.queue, 1, &submit, self.fence))) return;

    var present: c.VkPresentInfoKHR = .{
        .sType = c.VK_STRUCTURE_TYPE_PRESENT_INFO_KHR,
        .waitSemaphoreCount = 1,
        .pWaitSemaphores = &self.sem_finished,
        .swapchainCount = 1,
        .pSwapchains = &self.swapchain,
        .pImageIndices = &image_index,
    };
    const rc = c.vkQueuePresentKHR(self.queue, &present);
    if (rc == c.VK_ERROR_OUT_OF_DATE_KHR or rc == c.VK_SUBOPTIMAL_KHR) recreateSwapchain(self);
}

/// Converts the current draw rect into Vulkan NDC (Y down).
fn rectNdc(self: *VulkanEngine) [4]f32 {
    const n: render_common.NdcRect = render_common.ndcRect(self.draw_rect, self.sdl.logical_w, self.sdl.logical_h);
    return .{ n.x0, n.y_top, n.x1, n.y_bottom };
}

fn vt_set_fullscreen(ptr: *anyopaque, on: bool) void {
    const self: *VulkanEngine = as_self(ptr);
    self.sdl.setFullscreen(on);
    recreateSwapchain(self);
}

fn vt_set_vsync(ptr: *anyopaque, on: bool) void {
    const self: *VulkanEngine = as_self(ptr);
    self.vsync = on;
    recreateSwapchain(self);
}

fn vt_supports_curved_panorama(_: *anyopaque) bool {
    return false;
}

fn vt_supports_offscreen_targets(_: *anyopaque) bool {
    return false;
}

fn vt_create_texture(ptr: *anyopaque, width: u32, height: u32, _: ?[]const u8, _: u32) ?engine.TextureHandle {
    const self: *VulkanEngine = as_self(ptr);
    if (self.device == null) return null;
    if (!createTexture(self, width, height)) return null;
    return .{ .id = 1 };
}

fn vt_update_texture(ptr: *anyopaque, _: engine.TextureHandle, pixels: []const u8, pitch: u32) void {
    const self: *VulkanEngine = as_self(ptr);
    if (self.staging_size == 0) return;
    const w: usize = self.tex_w * 4;
    if (@as(usize, pitch) == w) {
        const n = @min(self.staging_size, pixels.len);
        @memcpy(self.staging_map[0..n], pixels[0..n]);
    } else {
        var row: usize = 0;
        while (row < self.tex_h) : (row += 1) {
            const src = row * pitch;
            const dst = row * w;
            if (src + w > pixels.len or dst + w > self.staging_size) break;
            @memcpy(self.staging_map[dst .. dst + w], pixels[src .. src + w]);
        }
    }
    self.tex_dirty = true;
}

fn vt_draw_texture(ptr: *anyopaque, _: engine.TextureHandle, dst: engine.Rect, _: ?engine.Rect, alpha: ?u8) void {
    const self: *VulkanEngine = as_self(ptr);
    self.draw_rect = dst;
    self.draw_alpha = if (alpha) |a| @as(f32, @floatFromInt(a)) / 255.0 else 1.0;
    self.has_draw = true;
}

fn vt_draw_texture_rotated(ptr: *anyopaque, tex: engine.TextureHandle, dst: engine.Rect, _: f32, alpha: ?u8) void {
    vt_draw_texture(ptr, tex, dst, null, alpha);
}

fn vt_texture_size(ptr: *anyopaque, _: engine.TextureHandle) engine.Point {
    const self: *VulkanEngine = as_self(ptr);
    return .{ .x = @intCast(self.tex_w), .y = @intCast(self.tex_h) };
}
