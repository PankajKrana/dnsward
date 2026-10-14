# Build:  rpmbuild -bb packaging/rpm/adblock.spec \
#           --define "_version 0.1.0" --define "_srcroot $PWD" --define "_bin $PWD/target/release/adblock"
Name:           adblock
Version:        %{_version}
Release:        1%{?dist}
Summary:        System-wide ad and tracker blocking
License:        MIT
URL:            https://github.com/PankajKrana/dnsward
Requires:       systemd
Requires(pre):  shadow-utils
%{?systemd_requires}

%description
AdBlock blocks advertising and tracking domains for the whole system with a
local filtering DNS resolver and provides browser extensions for in-page
filtering. No telemetry; statistics stay local. Run "sudo adblock enable"
after installation.

%install
install -D -m 0755 %{_bin} %{buildroot}%{_bindir}/adblock
ln -s adblock %{buildroot}%{_bindir}/adblock-native-host
install -d %{buildroot}%{_datadir}/adblock/filters
install -m 0644 %{_srcroot}/filters/*.txt %{buildroot}%{_datadir}/adblock/filters/
install -d %{buildroot}%{_unitdir}
sed 's#/usr/local/bin/adblock#%{_bindir}/adblock#g' %{_srcroot}/platforms/linux/adblock.service > %{buildroot}%{_unitdir}/adblock.service
install -D -m 0644 %{_srcroot}/crates/adblock-config/src/default-config.toml %{buildroot}%{_sysconfdir}/adblock/config.toml
install -d %{buildroot}%{_sharedstatedir}/adblock/data

%pre
getent passwd adblock >/dev/null || useradd -r -M -d %{_sharedstatedir}/adblock -s /sbin/nologin adblock
exit 0

%post
%systemd_post adblock.service
chown adblock:adblock %{_sharedstatedir}/adblock/data
%{_bindir}/adblock browser install --system >/dev/null 2>&1 || :
echo "AdBlock installed. Turn protection on with:  sudo adblock enable"

%preun
if [ $1 -eq 0 ]; then
    %{_bindir}/adblock disable >/dev/null 2>&1 || :
    %{_bindir}/adblock browser uninstall --system >/dev/null 2>&1 || :
fi
%systemd_preun adblock.service

%postun
%systemd_postun_with_restart adblock.service

%files
%license %{_srcroot}/LICENSE
%{_bindir}/adblock
%{_bindir}/adblock-native-host
%{_datadir}/adblock
%{_unitdir}/adblock.service
%dir %{_sysconfdir}/adblock
%config(noreplace) %{_sysconfdir}/adblock/config.toml
%dir %{_sharedstatedir}/adblock
%dir %{_sharedstatedir}/adblock/data
