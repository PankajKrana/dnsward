# Build:  rpmbuild -bb packaging/rpm/fuck-ads.spec \
#           --define "_version 0.1.0" --define "_srcroot $PWD" --define "_bin $PWD/target/release/fuck-ads"
# The release workflow builds this on Ubuntu, whose rpm has no systemd macros.
# These fallbacks match the standard Fedora/RHEL definitions and are skipped
# where the real macros exist.
%{!?_unitdir: %global _unitdir /usr/lib/systemd/system}
%{!?_licensedir: %global _licensedir %{_datadir}/licenses}
%{!?systemd_post: %global systemd_post() if [ $1 -eq 1 ]; then systemctl preset %{?*} >/dev/null 2>&1 || :; fi}
%{!?systemd_preun: %global systemd_preun() if [ $1 -eq 0 ]; then systemctl --no-reload disable --now %{?*} >/dev/null 2>&1 || :; fi}
%{!?systemd_postun_with_restart: %global systemd_postun_with_restart() systemctl daemon-reload >/dev/null 2>&1 || :; if [ $1 -ge 1 ]; then systemctl try-restart %{?*} >/dev/null 2>&1 || :; fi}

Name:           fuck-ads
Version:        %{_version}
Release:        1%{?dist}
Summary:        System-wide ad and tracker blocking
License:        MIT
URL:            https://github.com/PankajKrana/fuck-ads
Requires:       systemd
Requires(pre):  shadow-utils
%{?systemd_requires}

%description
FuckAds blocks advertising and tracking domains for the whole system with a
local filtering DNS resolver and provides browser extensions for in-page
filtering. No telemetry; statistics stay local. Run "sudo fuck-ads enable"
after installation.

%install
install -D -m 0755 %{_bin} %{buildroot}%{_bindir}/fuck-ads
ln -s fuck-ads %{buildroot}%{_bindir}/fuck-ads-native-host
install -d %{buildroot}%{_datadir}/fuck-ads/filters
install -m 0644 %{_srcroot}/filters/*.txt %{buildroot}%{_datadir}/fuck-ads/filters/
sh %{_srcroot}/browser/build.sh >/dev/null
cp -R %{_srcroot}/browser/dist/chromium %{_srcroot}/browser/dist/firefox %{buildroot}%{_datadir}/fuck-ads/
install -d %{buildroot}%{_unitdir}
sed 's#/usr/local/bin/fuck-ads#%{_bindir}/fuck-ads#g' %{_srcroot}/platforms/linux/fuck-ads.service > %{buildroot}%{_unitdir}/fuck-ads.service
install -D -m 0644 %{_srcroot}/crates/fuck-ads-config/src/default-config.toml %{buildroot}%{_sysconfdir}/fuck-ads/config.toml
install -d %{buildroot}%{_sharedstatedir}/fuck-ads/data
install -D -m 0644 %{_srcroot}/LICENSE %{buildroot}%{_licensedir}/fuck-ads/LICENSE

%pre
getent passwd fuck-ads >/dev/null || useradd -r -M -d %{_sharedstatedir}/fuck-ads -s /sbin/nologin fuck-ads
exit 0

%post
%systemd_post fuck-ads.service
chown fuck-ads:fuck-ads %{_sharedstatedir}/fuck-ads/data
%{_bindir}/fuck-ads browser install --system >/dev/null 2>&1 || :
echo "FuckAds installed. Turn protection on with:  sudo fuck-ads enable"

%preun
if [ $1 -eq 0 ]; then
    %{_bindir}/fuck-ads disable >/dev/null 2>&1 || :
    %{_bindir}/fuck-ads browser uninstall --system >/dev/null 2>&1 || :
fi
%systemd_preun fuck-ads.service

%postun
%systemd_postun_with_restart fuck-ads.service

%files
%license %{_licensedir}/fuck-ads/LICENSE
%{_bindir}/fuck-ads
%{_bindir}/fuck-ads-native-host
%{_datadir}/fuck-ads
%{_unitdir}/fuck-ads.service
%dir %{_sysconfdir}/fuck-ads
%config(noreplace) %{_sysconfdir}/fuck-ads/config.toml
%dir %{_sharedstatedir}/fuck-ads
%dir %{_sharedstatedir}/fuck-ads/data
