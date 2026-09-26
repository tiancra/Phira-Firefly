

use crate::lan::*;
use crate::charts_view::ChartsView;
use crate::scene::SongScene;
use anyhow::Result;
use macroquad::prelude::*;
use prpr::ui::Ui;
use std::sync::{Arc, Mutex};

pub struct LanIntegration {
    
    charts_view: ChartsView,
    
    lan_manager: Arc<Mutex<LanManager>>,
    
    lan_panel: LanPanel,
    
    waiting_panel: WaitingPanel,
    
    audio_panel: AudioDevicePanel,
    
    current_state: IntegrationState,
}

#[derive(Debug, Clone)]
pub enum IntegrationState {
    
    Main,
    
    LanPanel,
    
    Waiting,
    
    InGame,
    
    AudioDevices,
}

impl LanIntegration {
    
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

    
    pub fn show_lan_panel(&mut self) {
        self.current_state = IntegrationState::LanPanel;
        self.lan_panel.show(0.0);
    }

    
    pub fn show_waiting_panel(&mut self, total_count: u8) {
        self.current_state = IntegrationState::Waiting;
        self.waiting_panel.show(0.0, total_count);
    }

    
    pub fn show_audio_panel(&mut self, devices: Vec<String>) {
        self.current_state = IntegrationState::AudioDevices;
        self.audio_panel.show(devices);
    }

    
    pub fn update(&mut self, t: f32) -> Result<()> {
        match self.current_state {
            IntegrationState::Main => {
                
                self.charts_view.update(t)?;
            }
            IntegrationState::LanPanel => {
                
                self.lan_panel.update(t)?;
            }
            IntegrationState::Waiting => {
                
                self.waiting_panel.update(t)?;
            }
            IntegrationState::InGame => {
                
            }
            IntegrationState::AudioDevices => {
                
                self.audio_panel.update(t)?;
            }
        }

        
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

    
    pub fn render(&mut self, ui: &mut Ui, t: f32) {
        match self.current_state {
            IntegrationState::Main => {
                
                self.charts_view.render(ui, ui.screen_rect(), t);
            }
            IntegrationState::LanPanel => {
                
                self.lan_panel.render(ui, t);
            }
            IntegrationState::Waiting => {
                
                self.waiting_panel.render(ui, t);
            }
            IntegrationState::InGame => {
                
                
            }
            IntegrationState::AudioDevices => {
                
                self.audio_panel.render(ui, t);
            }
        }
    }

    
    pub fn touch(&mut self, touch: &Touch, t: f32) -> bool {
        match self.current_state {
            IntegrationState::Main => {
                
                self.charts_view.touch(touch, t, 0.0).unwrap_or(false)
            }
            IntegrationState::LanPanel => {
                
                self.lan_panel.touch(touch, t)
            }
            IntegrationState::Waiting => {
                
                self.waiting_panel.touch(touch, t)
            }
            IntegrationState::InGame => {
                
                false
            }
            IntegrationState::AudioDevices => {
                
                self.audio_panel.touch(touch, t)
            }
        }
    }

    
    pub fn handle_chart_menu_click(&mut self, chart_index: usize) {
        
        self.show_lan_panel();
    }

    
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

    
    pub fn join_room(&mut self, server_addr: String) -> Result<()> {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.join_room(server_addr, "Player".to_string())?;
        }
        Ok(())
    }

    
    pub fn ready(&mut self, ready: bool) -> Result<()> {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.ready(ready)?;
        }
        Ok(())
    }

    
    pub fn start_game(&mut self, waiting_for_players: bool) -> Result<()> {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.start_game(waiting_for_players)?;
        }
        Ok(())
    }

    
    pub fn update_audio_devices(&mut self, devices: Vec<AudioDeviceInfo>, selected_index: usize) -> Result<()> {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.update_audio_devices(devices, selected_index)?;
        }
        Ok(())
    }

    
    pub fn request_download_chart(&mut self, chart_id: String, chart_name: String) -> Result<()> {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.request_download_chart(chart_id, chart_name)?;
        }
        Ok(())
    }

    
    pub fn disconnect(&mut self) {
        if let Ok(mut manager) = self.lan_manager.lock() {
            manager.disconnect();
        }
    }

    
    pub fn get_current_state(&self) -> &IntegrationState {
        &self.current_state
    }
}

pub fn example_usage() {
    
    let icons = Arc::new(crate::icons::Icons::new());
    let rank_icons = [prpr::ext::SafeTexture::new(); 8]; 
    
    
    let mut integration = LanIntegration::new(icons, rank_icons);
    
    
    loop {
        
        if let Err(e) = integration.update(0.0) {
            println!("Update error: {}", e);
            break;
        }

        
        
        

        
        
        

        
    }
}
