# ESP32-S3 内部温度传感器实现

## 概述

ESP32-S3 内置一个 8 位 Sigma-Delta ADC 温度传感器，通过 SENS 外设寄存器控制，可测量芯片内部温度（范围 -20°C ~ 110°C）。

## 寄存器映射

| 寄存器 | 地址域 | 关键字段 | 描述 |
|--------|--------|----------|------|
| `SAR_TSENS_CTRL` | `SENS` | `sar_tsens_out[7:0]` | 温度数据输出 |
| | | `sar_tsens_ready[8]` | 数据就绪标志 |
| | | `sar_tsens_clk_div[21:14]` | 时钟分频 (默认 6) |
| | | `sar_tsens_power_up[22]` | 上电控制 |
| | | `sar_tsens_power_up_force[23]` | SW 强制上电 |
| | | `sar_tsens_dump_out[24]` | 触发读取 |
| `SAR_TSENS_CTRL2` | `SENS` | `sar_tsens_xpd_force[13:12]` | 启动等待时间 (0b11) |
| `SAR_PERI_CLK_GATE_CONF` | `SENS` | `tsens_clk_en[29]` | TSENS 时钟门控 |
| `I2C_SAR_REG7` | REGI2C 总线 | `ADC_SAR_ENT_TSENS[2]` | 使能模拟通路 |

## 操作流程

### 1. 时钟使能

```rust
SENS::regs()
    .sar_peri_clk_gate_conf()
    .modify(|_, w| w.tsens_clk_en().set_bit());
```

### 2. 上电

```rust
SENS::regs().sar_tsens_ctrl().modify(|_, w| {
    w.sar_tsens_power_up_force().set_bit()
     .sar_tsens_power_up().set_bit()
});
SENS::regs().sar_tsens_ctrl2()
    .modify(|_, w| unsafe { w.sar_tsens_xpd_force().bits(3) });
```

上电后需等待 300µs 使传感器稳定。

### 3. 读取

```rust
// 触发读取
regs.sar_tsens_ctrl()
    .modify(|_, w| w.sar_tsens_dump_out().set_bit());
// 等待就绪
while !regs.sar_tsens_ctrl().read().sar_tsens_ready().bit_is_set() {}
// 读数据
let raw = regs.sar_tsens_ctrl().read().sar_tsens_out().bits();
// 清除触发
regs.sar_tsens_ctrl()
    .modify(|_, w| w.sar_tsens_dump_out().clear_bit());
```

### 4. 温度换算

```
°C = raw_value × 0.4386 - offset × 27.88 - 20.52
```

无校准（DAC=0）时 offset ≈ 0，公式简化为：

```
°C = raw_value × 0.4386 - 20.52
```

原始值范围 0~255，例如 `raw=120` → `120 × 0.4386 - 20.52 ≈ 32.1°C`

## 精度说明

ESP32-S3 的温度传感器主要用于监测芯片温度变化趋势，出厂时每个芯片的 efuse 中烧录了校准值 `deltaT`。使用校准值的更精确公式为：

```
°C = raw_value - deltaT / 10
```

未使用校准值时误差约 ±5°C。

## 与 ESP-IDF 驱动对照

| 步骤 | 本实现 | ESP-IDF `temperature_sensor_ll.h` |
|------|--------|----------------------------------|
| 时钟使能 | `tsens_clk_en = 1` | `temperature_sensor_ll_bus_clk_enable(true)` |
| 复位 | `tsens_reset` 置位后清 | `temperature_sensor_ll_reset_module()` |
| 使能模拟通路 | REGI2C `ADC_SAR_ENT_TSENS` | `REGI2C_WRITE_MASK(I2C_SAR_ADC, I2C_SARADC_ENT_TSENS, 1)` |
| 设置范围 | REGI2C TSENS_DAC | `temperature_sensor_ll_set_range(dac)` |
| 上电 | `power_up = 1` | `temperature_sensor_ll_enable(true)` |
| 读取 | dump_out→ready→out | `temperature_sensor_ll_get_raw_value()` |

## RTT 重复显示问题

### 现象

首次烧录运行时，前几行数据显示在 `< 500ms` 内连续输出，伴有重复行，之后稳定在预期间隔。

### 原因

- `probe-rs` 在 macOS 上每次 RTT 轮询（~107ms 间隔）读取目标内存中的 RTT 缓冲区
- 首次 `cargo run` 编译耗时 ~20s，芯片在此期间持续写入 RTT 缓冲区
- RTT 缓冲区（默认 1024 字节）积压约 50 行数据
- probe-rs RTT 读取指针管理在 macOS 上存在重复读取问题

### 解决方法

1. **增加 `delay_millis(2000)`**：放慢写入速率，给 probe-rs 充足时间排空缓冲区
2. **使用缓存编译**：之后 `cargo run` 无需重新编译，芯片启动后 RTT 几乎立即连接，缓冲区无积压
3. **RTT 缓冲区模式**：若用 `rtt_init!` 直接初始化且选 `NoBlockTrim` 模式，可避免旧数据残留

### 确认方法

在 `rprintln!` 中添加序号：

```rust
static mut SEQ: u32 = 0;
let seq = unsafe { SEQ += 1; SEQ };
rprintln!("#{}  {}°C", seq, temp);
```

序号唯一递增说明芯片每秒确实只写了 1 行，是 host 端 RTT 读取导致重复。

## 依赖的 HAL 接口

- `esp_hal::peripherals::SENS::regs()` — 访问 SENS 寄存器块
- `esp_hal::delay::Delay` — 毫秒/微秒延时
- `rtt_target::{rprintln, rtt_init_print}` — RTT 输出

## 文件引用

- ESP32-S3 PAC: `esp32s3-0.35.2/src/sens/sar_tsens_ctrl.rs`
- ESP32-S3 PAC: `esp32s3-0.35.2/src/sens/sar_tsens_ctrl2.rs`
- ESP32-S3 PAC: `esp32s3-0.35.2/src/sens/sar_peri_clk_gate_conf.rs`
- ESP32-S3 PAC: `esp32s3-0.35.2/src/sens/sar_peri_reset_conf.rs`
- esp-hal REGI2C: `src/soc/esp32s3/regi2c.rs`
- ESP-IDF LL: `components/hal/esp32s3/include/hal/temperature_sensor_ll.h`
