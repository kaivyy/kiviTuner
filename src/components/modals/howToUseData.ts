/**
 * Structured workflow guide chapters and search/navigation helpers for the
 * interactive "How to Use kiviTuner" tutorial window.
 */

export type TutorialSceneId =
  | 'workspace'
  | 'explorer'
  | 'tables'
  | 'scalar-flag'
  | 'hex-reverse'
  | 'diff-checksum'
  | 'telemetry-ecu'
  | 'diag-moto-uds';

export interface TutorialCallout {
  pin: number;
  label: string;
  detail: string;
}

export interface TutorialShortcut {
  keys: string[];
  action: string;
}

export interface TutorialChapter {
  id: TutorialSceneId;
  index: number;
  shortTitle: string;
  title: string;
  subtitle: string;
  menuPath: string;
  figureCaption: string;
  figureAriaLabel: string;
  shortcuts: TutorialShortcut[];
  callouts: TutorialCallout[];
  steps: string[];
  tip: string;
  keywords: string[];
}

export const TUTORIAL_CHAPTERS: readonly TutorialChapter[] = [
  {
    id: 'workspace',
    index: 1,
    shortTitle: 'Mulai Cepat',
    title: 'Mulai Cepat & Navigasi Workspace',
    subtitle: 'Buka file ROM BIN, muat definisi XDF, lalu atur panel kerja utama dan tema.',
    menuPath: 'File -> Open ROM... / Definition (XDF) -> Load XDF...',
    figureCaption: 'Potongan atas Menu Bar Tahoe, Activity Rail kiri, dan Workspace Tab Bar.',
    figureAriaLabel:
      'Ilustrasi potongan workspace utama KiviTuner berisi Menu Bar, indikator Checksum, Activity Rail, dan tab peta.',
    shortcuts: [
      { keys: ['Ctrl', 'O'], action: 'Buka file ROM BIN' },
      { keys: ['Ctrl', 'D'], action: 'Muat file definisi XDF' },
      { keys: ['Ctrl', 'K'], action: 'Command Palette' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Menu Bar, Checksum & Update Pill',
        detail: 'Menampung 7 menu utama (termasuk Help -> Check for Updates...), indikator Checksum, dan Delta byte aktif.',
      },
      {
        pin: 2,
        label: 'Activity Rail Kiri',
        detail: 'Beralih cepat ke Map Explorer, Inspector, Discovery Scanner, Hex, Waveform, jendela pop-out OS terpisah, dan tema Dark/Light.',
      },
      {
        pin: 3,
        label: 'Workspace Tabs, Split View & Tab Pop-Out',
        detail: 'Kelola dokumen ROM, pin dan lepas tab ke floating window atau jendela OS terpisah tersinkron, serta atur lebar panel/split dengan menggeser pembatasnya.',
      },
    ],
    steps: [
      'Pilih File -> Open ROM... (Ctrl+O) untuk memuat file biner ECU (.bin) ke ruang kerja.',
      'Pilih Definition (XDF) -> Load XDF... (Ctrl+D) atau file .kivi.json agar daftar peta muncul otomatis.',
      'Gunakan Activity Rail di sisi kiri untuk membuka Map Explorer, Inspector, Hex, Waveform, atau jendela alat terpisah, lalu seret pembatas panel untuk mengatur lebarnya.',
      'Klik tombol detach di Workspace Tab Bar untuk memisahkan tab ke jendela mengambang/OS mandiri, atau tekan Ctrl+K (Command Palette) dan Help -> Check for Updates... untuk perintah cepat serta pembaruan.',
    ],
    tip: 'Pembatas Map Explorer, panel kanan, dan Split View dapat digeser bebas (double-click untuk reset; seret sangat sempit akan menutup panel otomatis) dengan tema Carbon Dark dan Lab Light.',
    keywords: ['open', 'rom', 'bin', 'xdf', 'tema', 'dark', 'light', 'command palette', 'workspace', 'tab', 'update'],
  },
  {
    id: 'explorer',
    index: 2,
    shortTitle: 'Map Explorer',
    title: 'Folder Map Explorer, Filter & Kandidat Peta',
    subtitle: 'Kelompokkan peta dalam folder lipat bertingkat, saring ukuran 2D/1D/1x1, dan buat definisi baru.',
    menuPath: 'Sidebar Kiri (Map Explorer) / Definition (XDF) -> Create New...',
    figureCaption: 'Potongan sidebar Map Explorer dengan folder lipat bertingkat, filter dimensi, dan Candidate Regions.',
    figureAriaLabel:
      'Ilustrasi potongan Map Explorer menampilkan kolom pencarian, filter All 2D 1D 1x1, pohon folder peta, dan Candidate Regions.',
    shortcuts: [
      { keys: ['Ctrl', 'F'], action: 'Jalankan Structural / Heuristic Scan' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Folder Lipat & Filter Dimensi',
        detail: 'Menata peta dalam pohon folder / kategori (Expand/Collapse All) serta menyaring matriks 2D, kurva 1D, atau skalar 1x1.',
      },
      {
        pin: 2,
        label: 'Tombol + Definisi Baru',
        detail: 'Membuat definisi Table 2D/3D, Axis 1D, Scalar 1x1, atau Bitmask Flag.',
      },
      {
        pin: 3,
        label: 'Candidate Regions (Auto-Detected)',
        detail: 'Daftar kandidat peta terstruktur dan heuristik siap klik ke Hex atau dipromosikan ke folder Auto-Detected.',
      },
    ],
    steps: [
      'Klik ikon folder atau tombol Expand All / Collapse All di header Map Explorer untuk membuka subkategori peta.',
      'Klik pill All, 2D, 1D, atau 1x1 (atau ketik di kotak cari) untuk menyaring tabel bahan bakar, kurva, atau skalar.',
      'Klik ikon + di header Map Explorer (atau menu Definition (XDF)) untuk menambah definisi peta baru.',
      'Buka panel Candidate Regions setelah scan selesai untuk meninjau kandidat tabel dan menambahkannya ke Map Explorer.',
    ],
    tip: 'Klik ikon Split pada item peta di Explorer untuk menaruh peta pembanding independen di panel kedua, lalu seret pembatasnya sesuai lebar kerja yang nyaman.',
    keywords: ['explorer', 'folder', 'filter', '2d', '1d', '1x1', 'candidate', 'potential maps', 'kategori', 'xdf', 'buat peta'],
  },
  {
    id: 'tables',
    index: 3,
    shortTitle: 'Tabel & Kurva',
    title: 'Edit Tabel 2D, Kurva 1D & Surface 3D',
    subtitle: 'Ubah nilai sel kalibrasi dengan matematika cepat, smoothing, dan Surface 3D ber-cage kalibrasi.',
    menuPath: 'Edit & Table Math / Toolbar Workspace (2D Table, Split 3D, Full 3D)',
    figureCaption: 'Potongan Heatmap Matrix 2D beserta toolbar operasi matematika dan pratinjau Surface 3D.',
    figureAriaLabel:
      'Ilustrasi potongan editor tabel 2D Heatmap lengkap dengan sel warna, seleksi blok, tombol math, dan Surface 3D.',
    shortcuts: [
      { keys: ['S'], action: 'Smooth 3x3 area terpilih' },
      { keys: ['I'], action: 'Interpolasi Bilinear 2D' },
      { keys: ['H'], action: 'Interpolasi Horizontal' },
      { keys: ['Ctrl', '\\'], action: 'Split 3D (2D + Surface 3D)' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Seleksi Blok & Edit Stabil Heatmap',
        detail: 'Klik dan seret beberapa sel pada grid 2D yang mudah digulir, atau klik ganda untuk mengetik nilai langsung tanpa menggeser lebar kolom.',
      },
      {
        pin: 2,
        label: 'Toolbar Table Math (+, -, %, =, S, I)',
        detail: 'Tambah/kurang konstanta, skalakan persentase, haluskan dengan Smooth 3x3, atau interpolasi antar tepi dengan strip tombol yang bisa digulir horizontal.',
      },
      {
        pin: 3,
        label: 'Surface 3D HUD & Kurva 1D Terkalibrasi',
        detail: 'Gunakan kontrol mengambang Camera (Iso/Top/Front/Side), Layers (Mesh/Solid/Wire/Cage/Ghost/Z-Scale), kapsul zoom, legenda warna ringkas, serta kurva 1D.',
      },
    ],
    steps: [
      'Pilih peta 2D atau 1D di Map Explorer untuk membukanya di area kerja utama, atau klik ikon lepas tab untuk membuka peta di jendela terpisah.',
      'Seret kursor pada sel Heatmap (atau klik ganda untuk edit angka langsung), lalu tekan +, -, %, atau = (maupun menu Edit & Table Math) untuk mengubah nilai massal.',
      'Gunakan tombol S (Smooth 3x3) atau I / H / V (Interpolate) agar transisi antarsel tetap halus.',
      'Tekan Split 3D atau Full 3D (Ctrl+\\), klik kontrol Camera/Layers di sudut kanvas 3D, seret pembatas split bila perlu, atau klik 2D Table kapan saja untuk kembali ke tabel penuh.',
    ],
    tip: 'Peta berdimensi 1xN atau Nx1 otomatis menampilkan editor kurva 1D lengkap dengan garis referensi stok putus-putus.',
    keywords: ['tabel', 'heatmap', '2d', '3d', 'surface', 'kurva', '1d', 'smooth', 'interpolasi', 'math'],
  },
  {
    id: 'scalar-flag',
    index: 4,
    shortTitle: 'Scalar 1x1 & Flag',
    title: 'Editor Scalar 1x1 & Sakelar Bitmask Flag',
    subtitle: 'Atur rev limiter, konstanta tunggal, dan switch bit 8-bit/16-bit tanpa salah hitung hex.',
    menuPath: 'Map Explorer -> Filter 1x1 / Definition (XDF) -> Create 1x1 Scalar / Bitmask Flag',
    figureCaption: 'Potongan ScalarFlagEditor dengan pembacaan Decimal, Raw Hex, Binary, dan sakelar bitmask.',
    figureAriaLabel:
      'Ilustrasi potongan Scalar dan Bitmask Flag Editor menampilkan nilai desimal, hex, biner, delta baseline, serta tombol bit 0 sampai 7.',
    shortcuts: [
      { keys: ['Ctrl', 'Z'], action: 'Undo perubahan terakhir' },
      { keys: ['Ctrl', 'Y'], action: 'Redo perubahan' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Nilai Terkalibrasi, Hex & Biner',
        detail: 'Ketiga representasi (Engineering, Raw Hex, Binary) tersinkron otomatis saat diedit.',
      },
      {
        pin: 2,
        label: 'Sakelar Bitmask (8-Bit / 16-Bit)',
        detail: 'Klik setiap kotak bit untuk menyalakan atau mematikan flag ECU individual.',
      },
      {
        pin: 3,
        label: 'Baseline Delta & Grid View',
        detail: 'Memantau selisih terhadap stok dan berpindah ke tampilan sel standar satu kali klik.',
      },
    ],
    steps: [
      'Klik filter 1x1 di Map Explorer atau buat parameter baru melalui Create 1x1 Scalar / Create Bitmask Flag.',
      'Ubah angka utama secara langsung untuk nilai skalar (misal batas RPM atau target AFR).',
      'Untuk peta flag, klik kotak Bit 0..Bit 7 (atau 16-bit) guna mengubah status bit tanpa menghitung mask manual.',
      'Klik tombol Grid View di pojok kanan atas editor apabila Anda lebih memilih mode sel tunggal klasik.',
    ],
    tip: 'Perhatikan baris Stock Baseline dan Delta di kartu editor untuk memastikan selisih angka tetap sesuai target sebelum menyimpan.',
    keywords: ['scalar', '1x1', 'bitmask', 'flag', 'switch', 'bit', 'hex', 'binary', 'grid view'],
  },
  {
    id: 'hex-reverse',
    index: 5,
    shortTitle: 'Hex Pro & Ledger',
    title: 'Hexdump Pro & Change Ledger',
    subtitle: 'Inspeksi byte mentah 8b/16b, navigasi kandidat peta, dan pantau setiap perubahan ROM.',
    menuPath: 'Tab Workspace -> Hex / Inspector Sidebar -> Change Ledger (vs Original)',
    figureCaption: 'Potongan Hexdump Pro toolbar, sorotan Diff & Potential Maps, Data Inspector, dan Change Ledger.',
    figureAriaLabel:
      'Ilustrasi potongan Hexdump Pro menampilkan kontrol 8b 16b Hex Dec, Diff hijau, kandidat peta amber, Data Inspector, dan Change Ledger.',
    shortcuts: [
      { keys: ['W'], action: 'Persempit kolom Hex' },
      { keys: ['M'], action: 'Perlebar kolom Hex' },
      { keys: ['F'], action: 'Lompat ke Potential Map berikut' },
      { keys: ['Shift', 'F'], action: 'Potential Map sebelumnya' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Kontrol 8b/16b, Hex/Dec & Diff',
        detail: 'Atur lebar kata, format angka, jumlah kolom 8B..32B, dan sorot hijau byte yang berubah.',
      },
      {
        pin: 2,
        label: 'Navigasi < Maps > & Promote Map',
        detail: 'Lompat ke blok amber hasil Auto-Scan lalu pilih rentang byte untuk dijadikan peta lewat Promote Map.',
      },
      {
        pin: 3,
        label: 'Inspector Change Ledger',
        detail: 'Daftar klaster byte termodifikasi terhadap ROM asli; klik baris mana saja untuk melompat ke alamatnya di Hex.',
      },
    ],
    steps: [
      'Buka tab Hex di Workspace Tab Bar (atau lepas ke jendela terpisah) maupun tekan ikon Hex di Activity Rail.',
      'Gunakan tombol 8b/16b, Hex/Dec, edit byte langsung pada sel hexdump, serta tombol -/+ kolom (atau hotkey W / M) agar pola struktur tabel terlihat jelas.',
      'Aktifkan tombol Diff untuk menandai byte yang berbeda dari ROM stok, dan tekan F / Shift+F untuk menelusuri Potential Maps.',
      'Seret rentang byte di hex grid (atau klik blok kandidat) lalu klik Promote Map untuk langsung membuat definisi tabel.',
    ],
    tip: 'Data Inspector di bagian bawah Hex viewer otomatis membaca nilai BE/LE u8, i8, u16, i16, u32, i32, hingga f32 pada kursor aktif.',
    keywords: ['hex', 'hexdump', 'hex editor', '8b', '16b', 'diff', 'promote map', 'change ledger', 'inspector', 'waveform'],
  },
  {
    id: 'diff-checksum',
    index: 6,
    shortTitle: 'Diff & Checksum',
    title: 'Binary Diff, Checksum & Simpan ROM',
    subtitle: 'Bandingkan revisi file biner, perbaiki checksum OEM, lalu simpan tanpa menimpa stok.',
    menuPath: 'Reverse Engineer -> Binary Diff Engine / Checksum & Integrity / File -> Save As...',
    figureCaption: 'Potongan tabel Binary Diff Engine bersama status verifikasi Checksum dan opsi Save As.',
    figureAriaLabel:
      'Ilustrasi potongan modal Binary Diff, kartu Verifikasi Checksum OEM, dan menu File Save As.',
    shortcuts: [
      { keys: ['Ctrl', 'B'], action: 'Buka Binary Diff Modal' },
      { keys: ['Ctrl', 'S'], action: 'Simpan ROM aktif' },
      { keys: ['Ctrl', 'Shift', 'S'], action: 'Save As salinan baru' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Binary Diff Modal',
        detail: 'Membandingkan ROM aktif dengan file pembanding serta mengekstrak area perubahan menjadi tabel baru.',
      },
      {
        pin: 2,
        label: 'Verify & Auto-Fix Checksum',
        detail: 'Menghitung ulang blok checksum sebelum file ditulis ulang ke ECU.',
      },
      {
        pin: 3,
        label: 'Save As Salinan Baru',
        detail: 'Menjaga file ROM stok asli tetap utuh sebagai cadangan pemulihan.',
      },
    ],
    steps: [
      'Buka Reverse Engineer -> Binary Diff Engine -> Open Diff Modal (Ctrl+B) untuk melihat daftar alamat ROM yang berbeda.',
      'Klik Extract jika ingin mengubah region hasil diff menjadi definisi peta kalibrasi baru.',
      'Periksa status Checksum di menu atas atau jalankan Reverse Engineer -> Checksum & Integrity -> Verify Checksum & Auto-Fix Checksum.',
      'Gunakan File -> Save As... (Ctrl+Shift+S) agar ROM hasil modifikasi tersimpan ke file terpisah dari ROM original.',
    ],
    tip: 'Tombol Revert Map to Stock di Inspector dapat mengembalikan nilai satu peta ke kondisi baseline ROM awal kapan pun dibutuhkan.',
    keywords: ['diff', 'binary diff', 'checksum', 'auto-fix', 'save', 'save as', 'revert', 'stock', 'oem', 'keihin'],
  },
  {
    id: 'telemetry-ecu',
    index: 7,
    shortTitle: 'Telemetry & Kabel',
    title: 'Live Telemetry RPM, Adapter Multi-Transport & Flash ECU',
    subtitle: 'Pantau RPM bergaya cockpit, pilih koneksi USB/Bluetooth/Wi-Fi/K-Line/OpenPort2/J2534, serta read/flash ECU ROM.',
    menuPath: 'Acquisition & ECU -> Live Telemetry & Auto-Tune / Flash ECU',
    figureCaption: 'Potongan panel Telemetry Dashboard (ARC/TRACK), koneksi Auto-Baud/Wi-Fi/K-Line, serta Read & Flash ROM.',
    figureAriaLabel:
      'Ilustrasi potongan kluster RPM Live Telemetry dan panel koneksi ELM327 USB Bluetooth Wi-Fi, OpenPort 2.0, dan Read Flash ECU.',
    shortcuts: [
      { keys: ['F9'], action: 'Mulai rekaman Datalog' },
      { keys: ['Esc'], action: 'Tutup jendela aktif teratas' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Supercar Cluster (ARC / TRACK)',
        detail: 'Menampilkan RPM, shift LED bar, gear, kecepatan, TPS, dan status AFR lewat ELM327 USB, Bluetooth SPP, atau Wi-Fi.',
      },
      {
        pin: 2,
        label: 'Multi-Transport & Probe Clone',
        detail: 'Auto-baud 38400/115200/9600 bps, 13 profil bus mobil dan motor, Wi-Fi TCP 192.168.0.10:35000, serta Raw K-Line FTDI.',
      },
      {
        pin: 3,
        label: 'Floating Multi-Window & Read/Flash',
        detail: 'Read dan Flash ROM lewat OpenPort 2.0, SAE J2534 (+ bridge 32-bit), SocketCAN ISO-TP, DoIP, atau Raw K-Line.',
      },
    ],
    steps: [
      'Buka Acquisition & ECU -> Live Telemetry & Auto-Tune -> Telemetry Dashboard (20Hz), lalu pilih adapter serial/Bluetooth SPP (/dev/rfcomm* atau COM*) atau target obd2_wifi:192.168.0.10:35000.',
      'Gunakan mode ARC atau TRACK dan muat sample/CSV datalog untuk menguji replay grafik serta review Auto-Tune sebelum menyambung kendaraan.',
      'Buka Acquisition & ECU -> Flash ECU -> klik Auto-Detect & Connect Cable atau pilih ELM327 Serial / ELM327 Wi-Fi (Auto Baud) / Raw K-Line USB / OpenPort 2.0 / J2534.',
      'Jika memakai dongle ELM327 murah, tekan Probe Clone untuk memeriksa dukungan perintah ATSH, ATCAF0, dan ATTP5 sebelum membaca atau meriset ECU.',
    ],
    tip: 'Adapter ELM327 (USB, Bluetooth SPP, maupun Wi-Fi) dikunci hanya untuk diagnosa & telemetry. Penulisan ROM ECU hanya aktif melalui OpenPort 2.0, SAE J2534, SocketCAN, DoIP, atau Raw K-Line.',
    keywords: ['telemetry', 'rpm', 'arc', 'track', 'elm327', 'bluetooth', 'wifi', 'auto-baud', 'probe', 'flash', 'openport', 'j2534', 'socketcan', 'k-line', 'window'],
  },
  {
    id: 'diag-moto-uds',
    index: 8,
    shortTitle: 'Diagnostik & Motor',
    title: 'Kokpit Diagnostik Multi-ECU, UDS Active Test & Motor',
    subtitle: 'Scan 54+ PID OBD-II, kamus DTC, scanner multi-ECU, servis UDS 0x10/0x22/0x31/0x2F, serta kabel modifikasi Honda/Yamaha/Kawasaki/Suzuki.',
    menuPath: 'Acquisition & ECU -> Flash ECU -> Tab Diagnostics (5 Sub-Tab)',
    figureCaption: 'Potongan 5 sub-tab Diagnostics Cockpit: Overview & DTCs, Live PIDs, UDS Service/Active Test, Motorcycle Lab, dan Raw Terminal.',
    figureAriaLabel:
      'Ilustrasi kokpit diagnostik 5 sub-tab menampilkan Multi-ECU Scanner, UDS Routine dan IOControl, Motorcycle Lab beserta panduan pinout kabel modifikasi, dan Raw Terminal.',
    shortcuts: [
      { keys: ['Enter'], action: 'Kirim perintah di Raw Terminal' },
    ],
    callouts: [
      {
        pin: 1,
        label: 'Overview, Multi-ECU & 54+ Live PIDs',
        detail: 'Scan 0x7E0 sampai 0x7E7 dan CAN 29-bit, baca Mode 02 Freeze Frame, I/M Readiness, Mode 0A/06, serta lookup kamus DTC offline.',
      },
      {
        pin: 2,
        label: 'UDS Session, Reset & Active Tests',
        detail: 'Masuk Session 0x10, baca DID 0x22, jalankan RoutineControl 0x31 dan IOControl 0x2F lengkap tombol Return Control to ECU (0x00).',
      },
      {
        pin: 3,
        label: 'Motorcycle Lab, Pinout & Raw Terminal',
        detail: 'Baca live sensor, kode kedip Honda 4-pin (Table 11/73/74/60), Yamaha/KDS/SDS, panduan pinout DLC modifikasi, dan konsol AT/OBD hex.',
      },
    ],
    steps: [
      'Masuk ke tab Diagnostics di panel Flash ECU, lalu tekan Scan All ECUs (0x7E0-0x7E7) di sub-tab Overview & DTCs untuk memilih ECU target, membaca baterai/VIN, DTC, Readiness, serta Freeze Frame.',
      'Buka sub-tab Live OBD PIDs untuk memindai seluruh sensor Mode 01 yang didukung ECU dan memantau hasil tes monitor Mode 06.',
      'Gunakan sub-tab UDS Service & Active Tests untuk berpindah sesi 0x10, membaca DID 0x22, atau menjalankan adaptasi 0x31 / aktuator 0x2F (selalu klik Return Control to ECU 0x00 setelah uji selesai).',
      'Untuk motor, buka sub-tab Motorcycle Lab, pilih Honda 4-pin K-Line, Euro 5 CAN, Yamaha, KDS, atau SDS, cocokkan pengkabelan modifikasi dengan Modified DLC Pinout Guide, lalu cek respon mentah lewat sub-tab Raw Terminal jika diperlukan.',
    ],
    tip: 'Dongle ELM327 v2.1 murah yang gagal Probe ATTP5 tetap berjalan di mobil/motor Euro 5 CAN, sedangkan motor Honda 4-pin (wakeup 70ms/120ms) dan Yamaha/KDS/SDS K-Line sebaiknya memakai kabel Raw K-Line FTDI KKL 409.1 atau Tactrix OpenPort 2.0.',
    keywords: ['diagnostik', 'multi-ecu', 'dtc', 'freeze frame', 'readiness', 'pid', 'uds', '0x31', '0x2f', 'active test', 'motor', 'honda', 'yamaha', 'kawasaki', 'kds', 'suzuki', 'sds', 'pinout', 'kabel modifikasi', 'terminal', 'raw', 'euro5'],
  },
];

/** Filters tutorial chapters across title, subtitle, menu path, steps, tip, and keywords. */
export function filterTutorialChapters(
  chapters: readonly TutorialChapter[],
  query: string
): TutorialChapter[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...chapters];

  return chapters.filter((ch) => {
    const haystack = [
      ch.shortTitle,
      ch.title,
      ch.subtitle,
      ch.menuPath,
      ch.tip,
      ...ch.keywords,
      ...ch.steps,
      ...ch.callouts.map((c) => `${c.label} ${c.detail}`),
    ]
      .join(' ')
      .toLowerCase();
    return haystack.includes(q);
  });
}

/** Resolves previous or next chapter ID within the provided ordered list. */
export function resolveAdjacentChapterId(
  chapters: readonly TutorialChapter[],
  currentId: TutorialSceneId,
  delta: -1 | 1
): TutorialSceneId | null {
  const idx = chapters.findIndex((c) => c.id === currentId);
  if (idx < 0) return null;
  const next = chapters[idx + delta];
  return next ? next.id : null;
}
