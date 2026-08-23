//! 局域网联机功能集成示例

use crate::lan::*;
use crate::charts_view::ChartsView;
use crate::scene::SongScene;
use anyhow::Result;
use macroquad::prelude::*;
use prpr::ui::Ui;
use std::sync::{Arc, Mutex};

/// 局域网联机集成示例
pub struct LanIntegration {
    /// 图表视图
    charts_view: ChartsView,
    /// 局域网管理器
    lan_manager: Arc<Mutex<LanManager>>,
    /// 局域网面板
    lan_panel: LanPanel,
    /// 等待面板
    waiting_panel: WaitingPanel,
    /// 音频设备面板
    audio_panel: AudioDevicePanel,
    /// 当前状态
    current_state: IntegrationState,
}

/// 集成状态
#[derive(Debug, Clone)]
pub enum IntegrationState {
    /// 主界面
    Main,
    /// 局域联机面板
    LanPanel,
    /// 等待其他玩家
    Waiting,
    /// 游戏中
    InGame,
    /// 自定义音频设备
    AudioDevices,
}

impl LanIntegration {
    /// 创建新的集成示例
    pub fn new(icons: Arc<crate::icons::Icons>, rank_icons: [prpr::ext::SafeTexture; 8]) -> Self {
        let lan_manager = Arc::new(Mutex::new(LanManager::new()));
        let charts_view = ChartsView::new(icons, rank_icons);
        
        Self {
            charts_view,
            lan_manager: lan_manager.clone(),
            lan_panel: LanPanel::new(lan_manager.clone()),
            waiting_panel: WaitingPanel::new(lan_manager.clone()),
            audio_panel: AudioDevicePanel::new(),
            current_state: IntegrationState::Main,
        }
    }

    /// 显示局域联机面板
    pub fn show_lan_panel(&mut self) {
        self.current_state = IntegrationState::LanPanel;
        self.lan_panel.show(0.0);
    }

    /// 显示等待面板
    pub fn show_waiting_panel(&mut self, total_count: u8) {
        self.current_state = IntegrationState::Waiting;
        self.waiting_panel.show(0.0, total_count);
    }

    /// 显示音频设备面板
    pub fn show_audio_panel(&mut self, devices: Vec<String>) {
        self.current_state = IntegrationState::AudioDevices;
        self.audio_panel.show(devices);
    }

    /// 更新集成
    pub fn update(&mut self, t: f32) -> Result<()> {
        match self.current_state {
            IntegrationState::Main => {
                // 更新图表视图
                self.charts_view.update(t)?;
            }
            IntegrationState::LanPanel => {
                // 更新局域联机面板
                self.lan_panel.update(t)?;
            }
            IntegrationState::Waiting => {
                // 更新等待面板
                self.waiting_panel.update(t)?;
            }
            IntegrationState::InGame => {
                // 游戏中，不需要更新
            }
            IntegrationState::AudioDevices => {
                // 更新音频设备面板
                self.audio_panel.update(t)?;
            }
        }

        // 检查状态变化
        if let Ok(manager) = self.lan_manager.lock() {
            match manager.get_state() {
                LanState::InGame { waiting_for_players, ready_players } => {
                    if waiting_for_players {
                        self.show_waiting_panel(ready_players.len() as u8);
                    } else {
                        self.current_state = IntegrationState::InGame;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// 渲染集成
    pub fn render(&mut self, ui: &mut Ui, t: f32) {
        match self.current_state {
            IntegrationState::Main => {
                // 渲染图表视图
                self.charts_view.render(ui, ui.screen_rect(), t);
            }
            IntegrationState::LanPanel => {
                // 渲染局域联机面板
                self.lan_panel.render(ui, t);
            }
            IntegrationState::Waiting => {
                // 渲染等待面板
                self.waiting_panel.render(ui, t);
            }
            IntegrationState::InGame => {
                // 游戏渲染
                // 这里应该渲染游戏界面
            }
            IntegrationState::AudioDevices => {
                // 渲染音频设备面板
                self.audio_panel.render(ui, t);
            }
        }
    }

    /// 处理触摸输入
    pub fn touch(&mut self, touch: &Touch, t: f32) -> bool {
        match self.current_state {
            IntegrationState::Main => {
                // 处理图表视图触摸
                self.charts_view.touch(touch, t, 0.0).unwrap_or(false)
            }
            IntegrationState::LanPanel => {
                // 处理局域联机面板触摸
                self.lan_panel.touch(touch, t)
            }
            IntegrationState::Waiting => {
                // 处理等待面板触摸
                self.waiting_panel.touch(touch, t)
            }
            IntegrationState::InGame => {
                // 游戏触摸处理
                false
            }
            IntegrationState::AudioDevices => {
                // 处理音频设备面板触摸
                self.audio_panel.touch(touch, t)
            }
        }
    }

    /// 处理图表菜单点击
    pub fn handle_chart_menu_click(&mut self, chart_index: usize) {
        // 显示局域联机面板
        self.show_lan_panel();
    }

    /// 创建房间
    pub fn create_room(&mut self) -> Result<()> {
        if let Ok(mut manager) = self.lan_manager.lock() {
            let config = LanConfig {
                server_name: "My Room".to_string(),
                room_name: "Test Room".to_string(),
                max_players: 4,
                waiting_for_players: false,
                local_ip: "0.0.0.0".to_string(),
                tcp_port: 27016,
            };
            
            manager.create_room(config)?;
        }
        Ok(())
    }

    /// 加入房间
    pub fn join_room(&mut self, server_addr: String) -> Result<()> {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.join_room(server_addr, "Player".to_string())?;
        }
        Ok(())
    }

    /// 准备就绪
    pub fn ready(&mut self, ready: bool) -> Result<()> {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.ready(ready)?;
        }
        Ok(())
    }

    /// 开始游戏
    pub fn start_game(&mut self, waiting_for_players: bool) -> Result<()> {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.start_game(waiting_for_players)?;
        }
        Ok(())
    }

    /// 更新音频设备设置
    pub fn update_audio_devices(&mut self, devices: Vec<AudioDeviceInfo>, selected_index: usize) -> Result<()> {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.update_audio_devices(devices, selected_index)?;
        }
        Ok(())
    }

    /// 请求下载谱面
    pub fn request_download_chart(&mut self, chart_id: String, chart_name: String) -> Result<()> {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.request_download_chart(chart_id, chart_name)?;
        }
        Ok(())
    }

    /// 断开连接
    pub fn disconnect(&mut self) {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.disconnect();
        }
    }

    /// 获取当前状态
    pub fn get_current_state(&self) -> &IntegrationState {
        &self.current_state
    }
}

/// 使用示例
pub fn example_usage() {
    // 初始化图标
    let icons = Arc::new(crate::icons::Icons::new());
    let rank_icons = [prpr::ext::SafeTexture::new(); 8]; // 简化处理
    
    // 创建集成实例
    let mut integration = LanIntegration::new(icons, rank_icons);
    
    // 游戏循环
    loop {
        // 更新
        if let Err(e) = integration.update(0.0) {
            println!("Update error: {}", e);
            break;
        }

        // 渲染
        // 这里需要实际的UI上下文
        // integration.render(ui, 0.0);

        // 处理输入
        // 这里需要实际的触摸输入
        // integration.touch(&touch, 0.0);

        // 其他游戏逻辑...
    }
}
