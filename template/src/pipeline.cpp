// 第八步：图形管线——着色器和全部固定功能状态打包成的一个不可变对象。
//
// 下面这两个头文件由构建系统生成：slangc 把 shaders/triangle.slang 编成
// SPIR-V，再转成 C 数组。内容是 TRIANGLE_VERT_SPV / TRIANGLE_FRAG_SPV 及其长度。
// 把字节码变成 VkShaderModule 的 create_shader_module() 在 shader.cpp 里。
#include "app.h"
#include "error.h"
#include "vertex.h"

#include "triangle_frag.spv.h"
#include "triangle_vert.spv.h"

// 创建图形管线：把着色器、顶点布局和所有固定功能状态一次性固化下来。
// 这个函数很长，因为 Vulkan 要求把每一项状态都写清楚，没有默认值。
bool Application::create_pipeline()
{
    // 着色器模块只在创建管线时用到，函数结尾就销毁。
    VkShaderModule vert_module = VK_NULL_HANDLE;
    VkShaderModule frag_module = VK_NULL_HANDLE;
    if (!create_shader_module(TRIANGLE_VERT_SPV, TRIANGLE_VERT_SPV_SIZE, &vert_module)
        || !create_shader_module(TRIANGLE_FRAG_SPV, TRIANGLE_FRAG_SPV_SIZE, &frag_module)) {
        return false;
    }

    // 输出色域用 specialization constant 交给着色器：它在整个程序生命周期里不变，
    // 所以没必要每帧用 push constant 送一遍。填进去之后，驱动编译这个管线时会把
    // color.slang 里那些 if 直接折叠掉，运行期一条分支都不剩。
    //
    // constantID 要和 color.slang 里 [[vk::constant_id(0)]] 的编号对上。
    const uint32_t gamut_value = static_cast<uint32_t>(gamut);
    VkSpecializationMapEntry gamut_entry{};
    gamut_entry.constantID = 0;
    gamut_entry.offset = 0;
    gamut_entry.size = sizeof(gamut_value);

    VkSpecializationInfo specialization{};
    specialization.mapEntryCount = 1;
    specialization.pMapEntries = &gamut_entry;
    specialization.dataSize = sizeof(gamut_value);
    specialization.pData = &gamut_value;

    // 两个可编程阶段。pName 是 SPIR-V 里的入口名，slangc 统一生成为 "main"。
    VkPipelineShaderStageCreateInfo stages[2]{};
    stages[0].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
    stages[0].stage = VK_SHADER_STAGE_VERTEX_BIT;
    stages[0].module = vert_module;
    stages[0].pName = "main";
    stages[0].pSpecializationInfo = &specialization;   // 色域只有顶点着色器用得到
    stages[1].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
    stages[1].stage = VK_SHADER_STAGE_FRAGMENT_BIT;
    stages[1].module = frag_module;
    stages[1].pName = "main";

    // binding 描述「一个顶点占多少字节」，attribute 描述「每个字段在哪、是什么格式」。
    // location 要和 shaders/triangle.slang 里 VertexInput 上的 [[vk::location(N)]] 对上。
    VkVertexInputBindingDescription binding{};
    binding.binding = 0;
    binding.stride = sizeof(Vertex);
    binding.inputRate = VK_VERTEX_INPUT_RATE_VERTEX;   // 每个顶点前进一步

    VkVertexInputAttributeDescription attributes[2]{};
    attributes[0].location = 0;
    attributes[0].binding = 0;
    attributes[0].format = VK_FORMAT_R32G32_SFLOAT;      // float2 position
    attributes[0].offset = offsetof(Vertex, position);
    attributes[1].location = 1;
    attributes[1].binding = 0;
    attributes[1].format = VK_FORMAT_R32G32B32_SFLOAT;   // float3 oklch
    attributes[1].offset = offsetof(Vertex, oklch);
    // 加新的顶点属性（UV、法线……）：在 Vertex 里加字段，这里加一条 attribute，
    // 把下面的 vertexAttributeDescriptionCount 一起改掉，着色器里也加对应的 location。

    VkPipelineVertexInputStateCreateInfo vertex_input{};
    vertex_input.sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO;
    vertex_input.vertexBindingDescriptionCount = 1;
    vertex_input.pVertexBindingDescriptions = &binding;
    vertex_input.vertexAttributeDescriptionCount = 2;
    vertex_input.pVertexAttributeDescriptions = attributes;

    // 顶点怎么组装成图元：每三个顶点一个三角形。
    VkPipelineInputAssemblyStateCreateInfo input_assembly{};
    input_assembly.sType = VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO;
    input_assembly.topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST;

    // 视口和裁剪矩形的具体数值是动态的（见下面的 dynamic_states），
    // 这里只声明各要一个。
    VkPipelineViewportStateCreateInfo viewport_state{};
    viewport_state.sType = VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO;
    viewport_state.viewportCount = 1;
    viewport_state.scissorCount = 1;

    // 光栅化：图元怎么变成像素。
    VkPipelineRasterizationStateCreateInfo rasterization{};
    rasterization.sType = VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO;
    rasterization.polygonMode = VK_POLYGON_MODE_FILL;          // 填充（改 LINE 可看线框）
    rasterization.cullMode = VK_CULL_MODE_NONE;                // 不剔除背面
    rasterization.frontFace = VK_FRONT_FACE_COUNTER_CLOCKWISE; // 逆时针为正面
    rasterization.lineWidth = 1.0f;

    // 多重采样抗锯齿，这里关着（每像素一个采样点）。
    VkPipelineMultisampleStateCreateInfo multisample{};
    multisample.sType = VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO;
    multisample.rasterizationSamples = VK_SAMPLE_COUNT_1_BIT;

    // 颜色混合：直接覆盖，RGBA 四个通道都写。
    // 做半透明时在这里打开 blendEnable 并配置混合因子。
    VkPipelineColorBlendAttachmentState blend_attachment{};
    blend_attachment.colorWriteMask = VK_COLOR_COMPONENT_R_BIT | VK_COLOR_COMPONENT_G_BIT
                                   | VK_COLOR_COMPONENT_B_BIT | VK_COLOR_COMPONENT_A_BIT;

    VkPipelineColorBlendStateCreateInfo color_blend{};
    color_blend.sType = VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO;
    color_blend.attachmentCount = 1;
    color_blend.pAttachments = &blend_attachment;

    // 列在这里的状态改成录制命令时再给，窗口缩放就不必重建管线。
    const VkDynamicState dynamic_states[] = {VK_DYNAMIC_STATE_VIEWPORT, VK_DYNAMIC_STATE_SCISSOR};
    VkPipelineDynamicStateCreateInfo dynamic_state{};
    dynamic_state.sType = VK_STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO;
    dynamic_state.dynamicStateCount = 2;
    dynamic_state.pDynamicStates = dynamic_states;

    // 管线布局声明着色器要读哪些外部资源。现在一个都没有，所以是空的。
    // 加 uniform 缓冲、纹理或 push constant 时，就在这里填。
    VkPipelineLayoutCreateInfo layout_info{};
    layout_info.sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO;
    VKX_CHECK(vkCreatePipelineLayout(device, &layout_info, nullptr, &pipeline_layout));

    // dynamic rendering 下没有 VkRenderPass，改成在这里告诉管线附件格式。
    VkPipelineRenderingCreateInfo rendering_info{};
    rendering_info.sType = VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO;
    rendering_info.colorAttachmentCount = 1;
    rendering_info.pColorAttachmentFormats = &swapchain_format;
    // 加深度缓冲时，depthAttachmentFormat 也在这里填，
    // 并给 pipeline_info 补一个 pDepthStencilState。

    VkGraphicsPipelineCreateInfo pipeline_info{};
    pipeline_info.sType = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO;
    pipeline_info.pNext = &rendering_info;
    pipeline_info.stageCount = 2;
    pipeline_info.pStages = stages;
    pipeline_info.pVertexInputState = &vertex_input;
    pipeline_info.pInputAssemblyState = &input_assembly;
    pipeline_info.pViewportState = &viewport_state;
    pipeline_info.pRasterizationState = &rasterization;
    pipeline_info.pMultisampleState = &multisample;
    pipeline_info.pColorBlendState = &color_blend;
    pipeline_info.pDynamicState = &dynamic_state;
    pipeline_info.layout = pipeline_layout;

    VkResult result =
        vkCreateGraphicsPipelines(device, VK_NULL_HANDLE, 1, &pipeline_info, nullptr, &pipeline);

    // 管线一旦建好，着色器模块就没用了。
    vkDestroyShaderModule(device, vert_module, nullptr);
    vkDestroyShaderModule(device, frag_module, nullptr);
    VKX_CHECK(result);
    return true;
}
