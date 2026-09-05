# -*- coding: utf-8 -*-
import io, os

base = 'phira/locales'
# 各语言翻译
translations = {
    'zh-CN': ('实时监测', '游戏中左上角显示FPS/CPU/内存/GPU/硬盘占用'),
    'zh-TW': ('即時監測', '遊戲中左上角顯示FPS/CPU/記憶體/GPU/硬碟占用'),
    'zh-LZH': ('實時監測', '局中左上顯FPS/CPU/內存/GPU/磁盤佔用'),
    'en-US': ('Performance Monitor', 'Show FPS/CPU/Memory/GPU/Disk usage in-game top-left'),
    'ja-JP': ('パフォーマンスモニター', 'ゲーム中左上にFPS/CPU/メモリ/GPU/ディスク使用率を表示'),
    'ko-KR': ('성능 모니터', '게임 내 좌측 상단에 FPS/CPU/메모리/GPU/디스크 사용량 표시'),
    'de-DE': ('Leistungsüberwachung', 'FPS/CPU/Arbeitsspeicher/GPU/Festplatte oben links im Spiel anzeigen'),
    'fr-FR': ('Moniteur de performances', 'Afficher FPS/CPU/Mémoire/GPU/Disque en haut à gauche en jeu'),
    'es-ES': ('Monitor de rendimiento', 'Mostrar FPS/CPU/Memoria/GPU/Disco arriba a la izquierda en el juego'),
    'pt-BR': ('Monitor de desempenho', 'Mostrar FPS/CPU/Memória/GPU/Disco no canto superior esquerdo'),
    'ru-RU': ('Монитор производительности', 'Показывать FPS/CPU/Память/GPU/Диск в левом верхнем углу'),
    'id-ID': ('Monitor Performa', 'Tampilkan FPS/CPU/Memori/GPU/Disk di kiri atas saat bermain'),
    'th-TH': ('ตรวจสอบประสิทธิภาพ', 'แสดง FPS/CPU/หน่วยความจำ/GPU/ดิสก์ที่มุมบนซ้ายในเกม'),
    'vi-VN': ('Giám sát hiệu năng', 'Hiển thị FPS/CPU/Bộ nhớ/GPU/Ổ đĩa góc trên trái trong game'),
    'tr-TR': ('Performans Monitörü', 'Oyun içinde sol üstte FPS/CPU/Bellek/GPU/Disk kullanımını göster'),
    'pl-PL': ('Monitor wydajności', 'Pokaż FPS/CPU/Pamięć/GPU/Dysk w lewym górnym rogu podczas gry'),
    'mn-MN': ('Гүйцэтгэлийн хянуур', 'Тоглоомын дундаа зүүн дээд буланд FPS/CPU/Санах ой/GPU/Дискийн хэмжээг харуулах'),
}

for lang, (title, sub) in translations.items():
    p = os.path.join(base, lang, 'settings.ftl')
    if not os.path.exists(p):
        print('skip', lang)
        continue
    with io.open(p, 'r', encoding='utf-8') as f:
        content = f.read()
    if 'item-perf-monitor' in content:
        print('exists', lang)
        continue
    # 确保文件末尾有换行
    if not content.endswith('\n'):
        content += '\n'
    content += f'item-perf-monitor = {title}\n'
    content += f'item-perf-monitor-sub = {sub}\n'
    with io.open(p, 'w', encoding='utf-8', newline='') as f:
        f.write(content)
    print('added', lang)
