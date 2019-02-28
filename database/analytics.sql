

-- population in each building by floor compared side-by side
with
  numbers(x) as (select 2 union select x+1 from numbers where x < 25),
  buildings(bdg, flr, cnt) as (
    select building_num, floor_num, count(distinct user_id)
      from comingouts
      group by building_num, floor_num)
  select
    printf('% 2s ', x),
    printf('% 2s ', b1.cnt),
    printf('% 2s ', b2.cnt),
    printf('% 2s ', b3.cnt),
    printf('% 2s ', b4.cnt)
    from numbers
      left join buildings b1 on (x = b1.flr and b1.bdg = 1)
      left join buildings b2 on (x = b2.flr and b2.bdg = 2)
      left join buildings b3 on (x = b3.flr and b3.bdg = 3)
      left join buildings b4 on (x = b4.flr and b4.bdg = 4)
;
